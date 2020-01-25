
extern crate hound;
extern crate minimp3;
extern crate opusfile;
extern crate claxon;

use mime_detective::MimeDetective;

use samplerate::{ConverterType, Samplerate, convert};

use std::thread::sleep;
use std::iter::{Iterator, FromIterator};
use std::result::Result;
use std::time::Duration;
use std::fs::File;

pub trait ReaderTarget<T> {
    fn read_iter <I: Iterator<Item = T>> (&mut self, iter: &mut I) -> usize;
    fn read_value (&mut self, val: T) -> Result<(), T>;
    fn is_full(&self) -> bool;
    fn ms_timing(&self) -> u64;
    fn single_ms_timing(&self) -> u64;

    fn read_iter_block <I: Iterator<Item = T>> (&mut self, iter: &mut I) {
        loop {
            if ! self.is_full() {
                let n = self.read_iter(iter);
                if n == 0 { break; }
            }
            else {
                sleep(Duration::from_millis(self.ms_timing()));
            }
        }
    }
    fn read_value_block (&mut self, val: T) {
        let mut v = val;
        loop {
            if self.is_full() {
                sleep(Duration::from_millis(self.single_ms_timing()));
            } else {
                match self.read_value(v) {
                    Err(x) => { v = x; },
                    Ok(_) => { break; }
                }
            }
        }
    }
}

pub struct Resampler<'a, T> {
    orig_rate: u32,
    dest_rate: u32,
    target: &'a mut T,
    converter: samplerate::Samplerate
}

impl<T: ReaderTarget<f32>> Resampler<'_, T> {
    fn resample (&mut self, input: &[f32]) {
        if self.dest_rate == self.orig_rate {
            self.target.read_iter_block(&mut input.iter().map(|x| *x));
        } else {
            let converted = self.converter.process(input).expect("couldn't resample");
            self.target.read_iter_block(&mut converted.iter().map(|x| *x));
        }
    }
}

pub trait AudioProducer : Sized {
    fn open(file_name: &str) -> Option<Self>;
    fn native_samplerate (&self) -> u32;
    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>);
    fn legnth (&self) -> u128;
    
    fn read_to_target <T: ReaderTarget<f32>> (&mut self, target: &mut T, sample_rate: u32) {
        let mut resampler = Resampler {
            orig_rate: self.native_samplerate(),
            dest_rate: sample_rate,
            target: target,
            converter: Samplerate::new(ConverterType::SincBestQuality, self.native_samplerate(), sample_rate, 2).expect("couldnt open converter")
        };
        self.read(&mut resampler);
    }
    
}

pub enum AudioFile {
    WavFile(WavReader),
    Mp3File(Mp3Reader),
    OpusFile(OpusReader),
    FlacFile(FlacReader)
}

impl AudioProducer for AudioFile {
    fn open(file_name: &str) -> Option<Self> {
        let struppi = MimeDetective::new().ok()?;
        let mime_type = struppi.detect_filepath(file_name).ok()?;
            
        if mime_type.type_() != mime::AUDIO {
            println!("not an audio file: {}", file_name);
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
    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>) {
        match self {
            AudioFile::Mp3File(f) => f.read(target),
            AudioFile::WavFile(f) => f.read(target),
            AudioFile::OpusFile(f) => f.read(target),
            AudioFile::FlacFile(f) => f.read(target)
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

    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>) {
        let chunk_len = self.native_samplerate();

        //TODO: check sample format
        let input = self.samples::<i16>().map(|x| x.expect("Error reading the testfile")); // TODO: propper error checking
        let mut samples = input.map(|x| (x as f32) / 32768.0);
        
        loop {
            let chunk = samples.by_ref().take(chunk_len as usize);
            if chunk.len() == 0 { break; }
            target.resample(& chunk.collect::<Vec<_>>());
        }
    }
}

pub struct Mp3Reader {
    decoder: minimp3::Decoder<std::fs::File>,
    sample_rate: u32
}

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

    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>) {
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
            
            target.resample(& curr_samples.collect::<Vec<_>>());

            if let Ok(n) = frame {
                let curr_samples = n.data.iter().map(|x| *x as f32 / 32768.0);
                target.resample(& curr_samples.collect::<Vec<_>>());
            }

            break;
        }

        while let Ok(n) = self.decoder.next_frame() {
            let curr_samples = n.data.iter().map(|x| *x as f32 / 32768.0);
            target.resample(& curr_samples.collect::<Vec<_>>());
        }
    }
}

pub type OpusReader = opusfile::Opusfile;

impl AudioProducer for OpusReader {
    fn open(file_name: &str) -> Option<Self> {
        opusfile::Opusfile::open(file_name).ok()
    }

    fn native_samplerate(&self) -> u32 { 48000 }

    fn legnth(&self) -> u128 { unimplemented!() }

    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>) {
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
                    target.resample(&samples)
                }
            }
        }
    }
}

type FlacReader = claxon::FlacReader<std::fs::File>;

impl AudioProducer for FlacReader {
    fn open(file_name: &str) -> Option<Self> {
        claxon::FlacReader::open(file_name).ok()
    }

    fn native_samplerate (&self) -> u32 {
        self.streaminfo().sample_rate
    }

    fn legnth(&self) -> u128 { unimplemented!() }

    fn read <T: ReaderTarget<f32>> (&mut self, target: &mut Resampler<T>) {
        let mut blocks = self.blocks();
        let mut buffer = Vec::new();

        while let Ok(Some(chunk)) = blocks.read_next_or_eof(buffer) {
            buffer = chunk.into_buffer();
            target.resample(& buffer.iter().map(|x| *x as f32 / 32768.0).collect::<Vec<_>>());
        }
    }
}