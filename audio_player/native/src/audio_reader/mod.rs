
extern crate hound;
extern crate minimp3;
extern crate opusfile;
extern crate claxon;

use mime_detective::MimeDetective;

use samplerate::{ConverterType, Samplerate, convert};

use std::{
    thread::sleep,
    iter::{Iterator, FromIterator},
    result::Result,
    time::Duration,
    fs::File,
    pin::Pin,
    task::{Context, Poll},
    future::Future,
    sync::{Arc, Mutex}
};

use futures::{
    sink::{Sink, SinkExt, Send},
    task::{Waker}
};

use async_trait::async_trait;

pub mod buffered_reader;

use buffered_reader::{BufferedReader, ReaderTarget};

pub struct Resampler<T> {
    orig_rate: u32,
    dest_rate: u32,
    target: BufferedReader<f32, T>,
    converter: samplerate::Samplerate
}

unsafe impl<T> std::marker::Send for Resampler <T> {}

impl<T: ReaderTarget<f32>> Resampler<T>
{
    fn resample (&mut self, input: &[f32]) -> impl Future + '_
    {
        let converted = {
            if self.dest_rate == self.orig_rate {
                Vec::from(input)
            } else {
                self.converter.process(input).expect("couldn't resample")
            }
        };

        self.target.send(converted)
    }
}

#[async_trait]
pub trait AudioProducer : Sized {
    fn open(file_name: &str) -> Option<Self>;
    fn native_samplerate (&self) -> u32;
    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>);
    fn legnth (&self) -> u128;    
}

pub fn resample_read <'a, T: ReaderTarget<f32> + 'a, P: AudioProducer> (
    prod: &'a mut P, 
    target: BufferedReader<f32, T>, 
    sample_rate: u32) -> impl Future + 'a
{
    let mut resampler = Resampler {
        orig_rate: prod.native_samplerate(),
        dest_rate: sample_rate,
        target: target,
        converter: {
            Samplerate::new(
                ConverterType::SincBestQuality, 
                prod.native_samplerate(), 
                sample_rate, 2)
            .expect("couldnt open converter")
        } 
    };
    prod.read(resampler)
}

pub enum AudioFile {
    WavFile(WavReader),
    Mp3File(Mp3Reader),
    OpusFile(OpusReader),
    FlacFile(FlacReader)
}

#[async_trait]
impl AudioProducer for AudioFile {
    fn open(file_name: &str) -> Option<Self> {
        let struppi = MimeDetective::new().ok()?;
        let mime_type = struppi.detect_filepath(file_name).ok()?;
            
        if mime_type.type_() != mime::AUDIO {
            println!("not an audio file: {} ({})", file_name, mime_type);
            return None;
        }

        match mime_type.subtype().as_str() {
            "mpeg" => {
                let file_reader = Mp3Reader::open(file_name)?;
                Some(AudioFile::Mp3File(file_reader))
            },
            "wav" | "x-wav" => {
                let file_reader = WavReader::open(file_name).ok()?;
                Some(AudioFile::WavFile(file_reader))
            },
            "ogg" => {
                let file_reader = OpusReader::open(file_name).ok()?;
                Some(AudioFile::OpusFile(file_reader))
            }
            "flac" | "x-flac" => {
                let file_reader = FlacReader::open(file_name).ok()?;
                Some(AudioFile::FlacFile(file_reader))
            }
            _ => { unimplemented!() },
        }
    }
    fn native_samplerate (&self) -> u32 {
        match self {
            AudioFile::Mp3File(f) => f.native_samplerate(),
            AudioFile::WavFile(f) => f.native_samplerate(),
            AudioFile::OpusFile(f) => f.native_samplerate(),
            AudioFile::FlacFile(f) => f.native_samplerate()
        }
    }
    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        match self {
            AudioFile::Mp3File(f) => f.read(target).await,
            AudioFile::WavFile(f) => f.read(target).await,
            AudioFile::OpusFile(f) => f.read(target).await,
            AudioFile::FlacFile(f) => f.read(target).await
        }
    }

    fn legnth(&self) -> u128 {
        match self {
            AudioFile::Mp3File(f) =>  f.legnth(),
            AudioFile::WavFile(f) =>  f.legnth(),
            AudioFile::OpusFile(f) => f.legnth(),
            AudioFile::FlacFile(f) => f.legnth()
        }
    }
}

pub type WavReader = hound::WavReader<std::io::BufReader<std::fs::File>>;

#[async_trait]
impl AudioProducer for WavReader {
    fn open(file_name: &str) -> Option<Self> {
        hound::WavReader::open(file_name).ok()
    }

    fn native_samplerate(&self) -> u32 {
        self.spec().sample_rate
    }

    fn legnth(&self) -> u128 {
        (self.len() / 2).into()
    }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        let chunk_len = self.native_samplerate();

        //TODO: check sample format
        let input = self.samples::<i16>().map(|x| x.expect("Error reading the testfile")); // TODO: propper error checking
        let mut samples = input.map(|x| (x as f32) / 32768.0);
        
        loop {
            let chunk = samples.by_ref().take(chunk_len as usize);
            if chunk.len() == 0 { break; }
            target.resample(& chunk.collect::<Vec<_>>()).await;
        }
    }
}

pub struct Mp3Reader {
    decoder: minimp3::Decoder<std::fs::File>,
    sample_rate: u32
}

#[async_trait]
impl AudioProducer for Mp3Reader {
    fn open(file_name: &str) -> Option<Self> {
        let f = File::open(file_name).ok()?;
        let mut dec = minimp3::Decoder::new(f);
        let r = dec.next_frame().ok()?.sample_rate;
        Some(Mp3Reader { decoder: dec, sample_rate: r as u32 })
    }
    fn native_samplerate(&self) -> u32 {
        self.sample_rate
    }

    fn legnth(&self) -> u128 { unimplemented!() }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        let mut frame = self.decoder.next_frame();

        while let Ok(f) = frame {
            frame = self.decoder.next_frame();

            if let Ok(n) = frame.as_ref() {
                if n.data.first() == Some(&0) { continue; }
            }

            let mut curr_samples = f.data.iter().map(|x| *x as f32 / 32768.0);

            while let (Some(l), Some(r)) = (curr_samples.next(), curr_samples.next()) {
                if l * l > 0.01 || r * r > 0.01 { break; }
            }
            
            target.resample(& curr_samples.collect::<Vec<_>>()).await;

            if let Ok(n) = frame {
                let curr_samples = n.data.iter().map(|x| *x as f32 / 32768.0);
                target.resample(& curr_samples.collect::<Vec<_>>()).await;
            }

            break;
        }

        while let Ok(n) = self.decoder.next_frame() {
            let curr_samples = n.data.iter().map(|x| *x as f32 / 32768.0);
            target.resample(& curr_samples.collect::<Vec<_>>()).await;
        }
    }
}

pub type OpusReader = opusfile::Opusfile;

#[async_trait]
impl AudioProducer for OpusReader {
    fn open(file_name: &str) -> Option<Self> {
        opusfile::Opusfile::open(file_name).ok()
    }

    fn native_samplerate(&self) -> u32 { 48000 }

    fn legnth(&self) -> u128 { unimplemented!() }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        loop {
            let mut buf = [0.0; 2000];
            match self.read_stereo(&mut buf) {
                Err(e) => {
                    println!("reading error opusfile: {:?}", e);
                    return;
                }
                Ok(n) => {
                    if n == 0 { break; }
                    let samples = buf.split_at(n*2).0;
                    target.resample(&samples).await;
                }
            }
        }
    }
}

type FlacReader = claxon::FlacReader<std::fs::File>;

#[async_trait]
impl AudioProducer for FlacReader {
    fn open(file_name: &str) -> Option<Self> {
        claxon::FlacReader::open(file_name).ok()
    }

    fn native_samplerate (&self) -> u32 {
        self.streaminfo().sample_rate
    }

    fn legnth(&self) -> u128 { unimplemented!() }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        let mut blocks = self.blocks();
        let mut buffer = Vec::new();

        while let Ok(Some(chunk)) = blocks.read_next_or_eof(buffer) {
            buffer = chunk.into_buffer();
            target.resample(& buffer.iter().map(|x| *x as f32 / 32768.0).collect::<Vec<_>>()).await;
        }
    }
}