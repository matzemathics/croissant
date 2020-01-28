#[macro_use]
extern crate neon;

use neon::prelude::*;

register_module!(mut cx, {
    cx.export_function("play", play)
      .and(cx.export_function("init", init))
});

extern crate cpal;
extern crate samplerate;
extern crate ringbuf;
extern crate mime_detective;
extern crate lazy_static;

mod audio_reader;

use audio_reader::{ReaderTarget, AudioFile, AudioProducer};

use cpal::traits::{HostTrait, EventLoopTrait};
use cpal::{StreamData, UnknownTypeOutputBuffer, Format};

use std::thread;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::boxed::Box;
use std::marker::PhantomData;

use ringbuf::{ RingBuffer, Producer };

use lazy_static::lazy_static;

//use id3::{Tag};

struct PlayerState<'a> {
    player: Option<CpalPlayer<'a>>
}

impl<'a> PlayerState<'a> {
    fn new() -> PlayerState<'a> {
        PlayerState { player: None }
    }

    fn initialized(&self) -> bool {
        self.player.is_none()
    }

    fn init(&mut self, player: CpalPlayer<'a>) {
        self.player = Some(player);
    }
}

struct CpalPlayer<'a> {
    event_loop: Arc<cpal::EventLoop>,
    stream_id: cpal::StreamId,
    sample_rate: u32,
    playing: bool,
    phantom: PhantomData<&'a ()>
}

fn stringify<T: Debug>(x: T) -> String { format!("Error - Debug: {:?}", x) }

impl CpalPlayer<'_> {
    fn new<'a>(
        sample_rate: u32, 
        mut cons: ringbuf::Consumer<f32>,
    ) -> Option<CpalPlayer<'a>>
    {
        let host = cpal::default_host();
        let event_loop = Arc::new(host.event_loop());
        let device = host.default_output_device()?;

        let format = Format {
            channels: 2,
            sample_rate: cpal::SampleRate(sample_rate),
            data_type: cpal::SampleFormat::F32
        };

        let stream_id = event_loop.build_output_stream(&device, &format).ok()?;

        let event_loop_copy = event_loop.clone();
        
        thread::spawn(move || {
            event_loop_copy.run(move |stream_id, stream_result| {
                let stream_data = match stream_result {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("an error occurred on stream {:?}: {}", stream_id, err);
                        return;
                    }
                };
    
                match stream_data {
                    StreamData::Output { buffer: UnknownTypeOutputBuffer::F32(mut buffer) } => {
                        let len = buffer.len();
                        for elem in buffer.iter_mut() {
                            *elem = cons.pop().unwrap_or(0.0);
                        }

                    },
                    _ => (),
                }
            });
        });
    
        Some(CpalPlayer {
            event_loop: event_loop,
            stream_id: stream_id,
            sample_rate: sample_rate,
            playing: true,
            phantom: PhantomData
        })
    }

    fn play(&self) -> Result<(), String> {
        self.event_loop.play_stream(self.stream_id.clone()).map_err(stringify)
    }

    fn pause(&self) -> Result<(), String> {
        self.event_loop.pause_stream(self.stream_id.clone()).map_err(stringify)
    }
}

lazy_static! { 
    static ref STATE: Arc<Mutex<PlayerState<'static>>> = {
        Arc::new(Mutex::new(PlayerState::new()))
    }; 
}

impl<T> ReaderTarget<T> for ringbuf::Producer<T> {
    fn read_iter <I: Iterator<Item=T>> (&mut self, iter: &mut I) -> usize {
        self.push_iter(iter)
    }

    fn read_value (&mut self, val: T) -> Result<(),T> {
        self.push(val)
    }

    fn is_full (&self) -> bool {
        self.is_full()
    }

    fn ms_timing (&self) -> u64 {
        let samples_per_second = 44100 * 2;
        let buffer_size = self.capacity() as u64;
        buffer_size * 1000 / (samples_per_second * 10)
    }

    fn single_ms_timing (&self) -> u64 { 1 }
}

fn init(mut cx: FunctionContext) -> JsResult<JsNull> {

    let sample_rate = 48000;
    let buffer_size = sample_rate as usize * 4;
    let auddiobuf = RingBuffer::<f32>::new(buffer_size);
    let (mut prod, mut cons) = auddiobuf.split();

    spawn_file_reader(prod, sample_rate);

    let res = cx.null();

    let player = CpalPlayer::new(sample_rate, cons).expect("cannot open player");
    player.pause();

    let mut state = STATE.lock().unwrap();
    state.init(player);

    Ok(res)
}

fn play(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    match &mut state.player {
        None => init(cx),
        Some(player) => {
            if player.playing {
                player.pause();
            } else {
                player.play();
            }
            player.playing = ! player.playing;
            Ok(cx.null())
        }
    }
}

fn spawn_file_reader (mut prod: Producer<f32>, sample_rate: u32) {

    thread::spawn(move || {
        let curr_dir = std::env::current_dir().unwrap();
        
        let files = vec![
            curr_dir.join("03_-_Rhizomes.opus")
        ];

        for file in files {
            if let Some(mut f) = AudioFile::open(file.to_str().unwrap()) {
                f.read_to_target(&mut prod, sample_rate)
            } else {
                println!("an error occurred while reading from {}", file.to_str().unwrap());
            }
        }
    });
}