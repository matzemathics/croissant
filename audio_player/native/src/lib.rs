#[macro_use]
extern crate neon;

use neon::prelude::*;

register_module!(mut cx, {
    cx.export_function("play", play)
      .and(cx.export_function("init", init))
      .and(cx.export_function("add_to_queue", add_pl))
      .and(cx.export_function("import_m3u", import_m3u))
      .and(cx.export_function("skip", skip))
});

extern crate cpal;
extern crate samplerate;
extern crate ringbuf;
extern crate mime_detective;
extern crate lazy_static;
extern crate m3u;
extern crate futures_util;

mod audio_reader;

use audio_reader::{ReaderTarget, AudioFile, AudioProducer, read_to_target};

use cpal::traits::{HostTrait, EventLoopTrait};
use cpal::{StreamData, UnknownTypeOutputBuffer, Format};

use std::{
    thread,
    thread::sleep,
    time::Duration,
    fmt::Debug,
    sync::{Arc, Mutex},
    boxed::Box,
    marker::PhantomData,
    collections::VecDeque,
    io::{BufReader},
    fs::File    
};

use futures::prelude::*;
use futures::future::{Abortable, AbortHandle, Aborted};
use futures::executor::block_on;

use ringbuf::{ RingBuffer, Producer };

use lazy_static::lazy_static;

use m3u::{Entry, EntryReader, Url};

//use id3::{Tag};

struct PlayerState<'a> {
    player: Option<CpalPlayer<'a>>,
    playlist: VecDeque<String>,
    skip_flag: bool,
}

impl<'a> PlayerState<'a> {
    fn new() -> PlayerState<'a> {
        PlayerState { player: None, playlist: VecDeque::new(), skip_flag: false }
    }

    fn initialized(&self) -> bool {
        self.player.is_none()
    }

    fn init(&mut self, player: CpalPlayer<'a>) {
        self.player = Some(player);
    }

    fn skip(&mut self) {
        self.skip_flag = true;
    }

    fn next_song(&mut self) -> Option<String> {
        self.playlist.pop_front()
    }

    fn add_to_queue(&mut self, title: String) {
        self.playlist.push_back(title);
    }

    fn play_next(&mut self, title: String) {
        self.playlist.push_front(title);
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

fn import_m3u (mut cx: FunctionContext) -> JsResult<JsNull> {
    if let Ok(arg0) = cx.argument::<JsString>(0) {
        let path = arg0.value();
        let mut reader = EntryReader::open(path.clone()).unwrap();
        let base = Url::from_file_path(path.as_str()).unwrap();

        for entry in reader.entries() {
            if let Ok(Entry::Path(p)) = entry {
                let p = base.join((*p).to_str().unwrap()).unwrap().to_file_path().unwrap();
                STATE.lock().unwrap().add_to_queue(p.to_str().unwrap().to_string());
            }
        }
    }
    Ok(cx.null())
}

fn add_pl (mut cx: FunctionContext) -> JsResult<JsNull> {
    if let Ok(arg0) = cx.argument::<JsString>(0) {
        STATE.lock().unwrap().add_to_queue(arg0.value());
    }
    Ok(cx.null())
}

fn skip (mut cx: FunctionContext) -> JsResult<JsNull> {
    STATE.lock().unwrap().skip();
    Ok(cx.null())
}

fn spawn_file_reader (mut prod: Producer<f32>, sample_rate: u32) {

    thread::spawn(move || {
        let curr_dir = std::env::current_dir().unwrap();
        
        loop {
            let mut guard = STATE.lock().unwrap();
            if let Some(file) = guard.next_song() {
                if let Some(mut f) = AudioFile::open(file.as_str()) {
                    let future = read_to_target(&mut f, &mut prod, sample_rate);
                    
                    while std::future::poll_with_tls_context(future) == std::task::Poll::Pending {
                        thread::sleep(Duration::from_millis(10));
                    }

                } else {
                    println!("an error occurred while reading from {}", file.as_str());
                }
            } else { 
                std::mem::drop(guard);
                sleep(Duration::from_millis(100));
            }
        }
    });
}
