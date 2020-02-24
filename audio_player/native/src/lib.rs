#[macro_use]
extern crate neon;

use neon::prelude::*;

register_module!(mut cx, {
    cx.export_function("play", play)
      .and(cx.export_function("pause", pause))
      .and(cx.export_function("init", init))
      .and(cx.export_function("add_to_queue", add_pl))
      .and(cx.export_function("import_m3u", import_m3u))
      .and(cx.export_function("skip", abort_curr))
      .and(cx.export_function("curr_playing", curr_playing))
      .and(cx.export_function("prev", prev))
});

extern crate cpal;
extern crate samplerate;
extern crate ringbuf;
extern crate mime_detective;
extern crate lazy_static;
extern crate m3u;
extern crate futures_util;

mod audio_reader;

use audio_reader::buffered_reader::{BufferedReader, ReaderTarget};
use audio_reader::{AudioFile, AudioProducer, resample_read};

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

use futures::{
    prelude::*,
    future::{Abortable, AbortHandle},
    executor::block_on,
    task::AtomicWaker
};

use ringbuf::{ RingBuffer, Producer };

use lazy_static::lazy_static;

use m3u::{Entry, EntryReader, Url};

//use id3::{Tag};

struct PlayerState<'a> {
    player: Option<CpalPlayer<'a>>,
    play_queue: VecDeque<String>,
    played_list: Vec<String>,
    abort_handle: Option<AbortHandle>
}

impl<'a> PlayerState<'a> {
    fn new() -> PlayerState<'a> {
        PlayerState { 
            player: None, 
            played_list: Vec::new(), 
            play_queue: VecDeque::new(), 
            abort_handle: None
        }
    }

    fn initialized(&self) -> bool {
        self.player.is_none()
    }

    fn init(&mut self, player: CpalPlayer<'a>) {
        self.player = Some(player);
    }

    fn abort_curr(&mut self) {
        if let Some(handle) = &self.abort_handle {
            handle.abort();
        }
    }

    fn advance(&mut self) -> Option<String> {
        let res = self.play_queue.pop_front();
        if let Some(p) = res.clone() { self.played_list.push(p); }
        res
    }

    fn go_back(&mut self) {
        let curr = self.played_list.pop();
        let res = self.played_list.pop();
        if let Some(p) = res.clone() { 
            self.play_queue.push_front(curr.unwrap());
            self.play_queue.push_front(p); 
        } else if let Some(p) = curr {
            self.play_queue.push_front(p);
        }
    }

    fn curr_playing(&self) -> Option<String> {
        self.played_list.first().map(|x| (*x).clone())
    }

    fn add_to_queue(&mut self, title: String) {
        self.play_queue.push_back(title);
    }

    fn play_next(&mut self, title: String) {
        self.play_queue.push_front(title);
    }

    fn rm_curr (&mut self) {
        self.played_list.pop();
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
        shared_waker: Arc<AtomicWaker>
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
                        shared_waker.wake();
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

    fn play(&mut self) -> Result<(), String> {
        self.playing = true;
        self.event_loop.play_stream(self.stream_id.clone()).map_err(stringify)
    }

    fn pause(&mut self) -> Result<(), String> {
        self.playing = false;
        self.event_loop.pause_stream(self.stream_id.clone()).map_err(stringify)
    }
}

lazy_static! { 
    static ref STATE: Arc<Mutex<PlayerState<'static>>> = {
        Arc::new(Mutex::new(PlayerState::new()))
    }; 
}

impl<T: Send> ReaderTarget<T> for ringbuf::Producer<T> {
    fn read_iter <I: Iterator<Item=T>> (&mut self, iter: &mut I) -> usize {
        self.push_iter(iter)
    }

    fn read_value (&mut self, val: T) -> Result<(),T> {
        self.push(val)
    }

    fn is_full (&self) -> bool {
        self.is_full()
    }
}

fn init(mut cx: FunctionContext) -> JsResult<JsNull> {

    let sample_rate = 48000;
    let buffer_size = sample_rate as usize * 4;
    let auddiobuf = RingBuffer::<f32>::new(buffer_size);
    let (mut prod, mut cons) = auddiobuf.split();

    let shared_waker = Arc::new(AtomicWaker::new());

    spawn_file_reader(prod, sample_rate, shared_waker.clone());

    let mut player = CpalPlayer::new(sample_rate, cons, shared_waker).expect("cannot open player");
    player.pause();

    let mut state = STATE.lock().unwrap();
    state.init(player);

    let val = cx.string("works");
    let global = cx.global();
    global.set(&mut cx, "test_audio_rust", val);

    Ok(cx.null())
}

fn play(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    match &mut state.player {
        None => init(cx),
        Some(player) => {
            player.play();
            Ok(cx.null())
        }
    }
}

fn pause(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    match &mut state.player {
        None => init(cx),
        Some(player) => {
            player.pause();
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

fn abort_curr (mut cx: FunctionContext) -> JsResult<JsNull> {
    STATE.lock().unwrap().abort_curr();
    Ok(cx.null())
}

fn prev (mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    state.go_back();
    state.abort_curr();
    Ok(cx.null())
}

fn curr_playing (mut cx: FunctionContext) -> JsResult<JsValue> {
    let p = STATE.lock().unwrap().curr_playing();

    if let Some(path) = p {
        let res = cx.string(path);
        Ok(res.as_value(&mut cx))
    } else {
        let res = cx.null();
        Ok(res.as_value(&mut cx))
    }
}

fn spawn_file_reader (mut prod: Producer<f32>, sample_rate: u32, shared_waker: Arc<AtomicWaker>) {

    thread::spawn(move || {
        let curr_dir = std::env::current_dir().unwrap();
        let prod = Arc::new(Mutex::new(prod));
        loop {
            let mut guard = STATE.lock().unwrap();
            if let Some(file) = guard.advance() {
                if let Some(mut f) = AudioFile::open(file.as_str()) {
                    let reader = BufferedReader::new(prod.clone(), shared_waker.clone());
                    let future = resample_read(&mut f, reader, sample_rate);

                    let (abort_handle, abort_reg) = AbortHandle::new_pair();
                    let future = Abortable::new(future, abort_reg);
                    guard.abort_handle = Some(abort_handle);

                    drop(guard);
                    println!("playing {}", file);
                    block_on(future);

                } else {
                    guard.rm_curr();
                    println!("an error occurred while reading from {}", file.as_str());
                }
            } else { 
                std::mem::drop(guard);
                sleep(Duration::from_millis(100));
            }
        }
    });
}
