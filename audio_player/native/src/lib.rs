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

use ringbuf::RingBuffer;

use lazy_static::lazy_static;

//use id3::{Tag};

type JsPlayer = CpalPlayer<Arc<Mutex<JsFunction<JsObject>>>>;

struct PlayerState {
    //player: Option<JsPlayer>
}

impl PlayerState {
    fn new() -> PlayerState {
        PlayerState { }//player: None }
    }

    fn initialized(&self) -> bool {
        true//self.player.is_none()
    }

    fn init(&mut self, player: JsPlayer) {
        //self.player = Some(player);
    }
}

struct CpalPlayer<T> {
    event_loop: Arc<cpal::EventLoop>,
    stream_id: cpal::StreamId,
    sample_rate: u32,
    callback: T,
    playing: bool
}

fn stringify<T: Debug>(x: T) -> String { format!("Error - Debug: {:?}", x) }

impl<'a, T: Callable<'a> + Clone> CpalPlayer<Arc<Mutex<T>>> {
    fn new(
        sample_rate: u32, 
        mut cons: ringbuf::Consumer<f32>, 
        cb: T, 
        argument: T::Argument
    ) -> Option<CpalPlayer<Arc<Mutex<T>>>>
    {
        let host = cpal::default_host();
        let event_loop = Arc::new(host.event_loop());
        let device = host.default_output_device()?;

        let callback = Arc::new(Mutex::new(cb));

        let format = Format {
            channels: 2,
            sample_rate: cpal::SampleRate(sample_rate),
            data_type: cpal::SampleFormat::F32
        };

        let stream_id = event_loop.build_output_stream(&device, &format).ok()?;

        let event_loop_copy = event_loop.clone();
        
        {
            let callback = Arc::clone(&callback);
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
                                let cb = callback.lock().unwrap();
                                (*cb).call(argument);
                            }
    
                        },
                        _ => (),
                    }
                });
            });
        }
    
        Some(CpalPlayer {
            event_loop: event_loop,
            stream_id: stream_id,
            sample_rate: sample_rate,
            callback: callback,
            playing: true
        })
    }

    fn play(&self) -> Result<(), String> {
        self.event_loop.play_stream(self.stream_id.clone()).map_err(stringify)
    }

    fn pause(&self) -> Result<(), String> {
        self.event_loop.pause_stream(self.stream_id.clone()).map_err(stringify)
    }
}

trait Callable<'a> {
    type Result;
    type Argument;
    fn call(self, arg: Self::Argument) -> Self::Result;
}

impl<'a, CL: Object> Callable<'a> for JsFunction<CL> {
    type Result = JsResult<'a, JsValue>;
    type Argument = &'a mut FunctionContext<'a>;

    fn call(self, arg: &'a mut FunctionContext) -> JsResult<'a, JsValue> {
        let args = std::iter::empty::<neon::handle::Handle<'_,JsValue>>();
        let this = arg.this();
        self.call(arg, this, args)
    }
}

lazy_static! { 
    static ref STATE: Arc<Mutex<PlayerState>> = {
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
    if cx.len() == 0 {
        let msg = cx.string("not enough arguments");
        return cx.throw(msg);
    }

    let callback = cx.argument_opt(0).unwrap();

    if ! callback.is_a::<JsFunction>() {
        let msg = cx.string("not enough arguments");
        return cx.throw(msg);
    }

    let sample_rate = 48000;
    let buffer_size = sample_rate as usize * 4;
    let auddiobuf = RingBuffer::<f32>::new(buffer_size);
    let (mut prod, mut cons) = auddiobuf.split();

    let player = CpalPlayer::new(sample_rate, cons).expect("cannot open player");
    player.pause();

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

    let mut state = STATE.lock().unwrap();
    state.init(player);

    Ok(cx.null())
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
