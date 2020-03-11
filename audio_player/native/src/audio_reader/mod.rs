
extern crate hound;
extern crate minimp3;
extern crate opusfile;
extern crate claxon;
extern crate id3;

use mime_detective::MimeDetective;

use samplerate::{ConverterType, Samplerate};

use std::{
    iter::Iterator,
    fs::File,
    future::Future,
};

use futures::sink::SinkExt;

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

    fn ready (&self) -> bool {
        self.target.ready()
    }
}

#[async_trait]
pub trait AudioProducer : Sized {
    fn open(file_name: &str) -> Option<Self>;
    fn native_samplerate (&self) -> u32;
    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>);
    fn legnth (&self) -> u32;    
}

pub fn resample_read <'a, T: ReaderTarget<f32> + 'a, P: AudioProducer> (
    prod: &'a mut P, 
    target: BufferedReader<f32, T>, 
    sample_rate: u32) -> impl Future + 'a
{
    let resampler = Resampler {
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

pub enum AudioFile<'a> 
{
    WavFile(WavReader),
    Mp3File(Mp3Reader),
    OpusFile(OpusReader<'a>),
    FlacFile(FlacReader)
}

#[async_trait]
impl AudioProducer for AudioFile<'_> {
    fn open(file_name: &str) -> Option<Self> {
        let struppi = MimeDetective::new().ok()?;
        let mime_type = struppi.detect_filepath(file_name).ok()?;
            
        let mut guessed_type = mime_type.subtype().as_str();

        if mime_type.type_() != mime::AUDIO {

            if file_name.ends_with(".mp3") { guessed_type = "mpeg" }
            else if file_name.ends_with(".wav") { guessed_type = "wav" }
            else if file_name.ends_with(".opus") { guessed_type = "ogg" }
            else {
                println!("not an audio file: {} ({})", file_name, mime_type); 
                return None; 
            }
        }

        match guessed_type {
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
    async fn read <T: ReaderTarget<f32>> (&mut self, target: Resampler<T>) {
        match self {
            AudioFile::Mp3File(f) => f.read(target).await,
            AudioFile::WavFile(f) => f.read(target).await,
            AudioFile::OpusFile(f) => f.read(target).await,
            AudioFile::FlacFile(f) => f.read(target).await
        }
    }

    fn legnth(&self) -> u32 {
        match self {
            AudioFile::Mp3File(f) =>  f.legnth(),
            AudioFile::WavFile(f) =>  f.legnth(),
            AudioFile::OpusFile(f) => f.legnth(),
            AudioFile::FlacFile(f) => f.legnth()
        }
    }
}

impl Tagged for AudioFile<'_> {
    fn tags(&self) -> Tags {
        match self {
            AudioFile::Mp3File(f) =>  (f as &dyn Tagged).tags(),
            AudioFile::WavFile(f) =>  (f as &dyn Tagged).tags(),
            AudioFile::OpusFile(f) => (f as &dyn Tagged).tags(),
            AudioFile::FlacFile(f) => (f as &dyn Tagged).tags()
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

    fn legnth(&self) -> u32 {
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

impl Tagged for WavReader {
    fn tags(&self) -> Tags {
        //TODO: figure out wave
        Tags::empty()
    }
}

pub struct Mp3Reader {
    decoder: minimp3::Decoder<std::fs::File>,
    sample_rate: u32,
    tags: Tags
}

impl Mp3Reader {
    // find the first frame containing audio
    // to reduce gaps when gepless playback is
    // demanded
    fn first_frame (&mut self) -> Vec<f32> {
        let mut decoded = Vec::new();
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
            
            let mut begin : Vec<f32> = curr_samples.collect();
            decoded.append(&mut begin);
    
            if let Ok(n) = frame {
                let mut curr_samples : Vec<f32> 
                    = n.data.iter()
                        .map(|x| *x as f32 / 32768.0)
                        .collect();
                
                decoded.append(&mut curr_samples);
            }
    
            return decoded;
        }

        panic!("should not be reached");
    }
}

#[async_trait]
impl AudioProducer for Mp3Reader {
    fn open(file_name: &str) -> Option<Self> {
        let tag = id3::Tag::read_from_path(file_name).ok()?;
        let tags = Tags {
            artist: tag.artist().unwrap_or("").to_owned(),
            album: tag.album().unwrap_or("").to_owned(),
            title: tag.title().unwrap_or("").to_owned()
        };
        drop(tag);

        let f = File::open(file_name).ok()?;
        let mut dec = minimp3::Decoder::new(f);
        let r = dec.next_frame().ok()?.sample_rate;

        Some(Mp3Reader { 
            decoder: dec,
            sample_rate: r as u32,
            tags: tags
        })
    }
    fn native_samplerate(&self) -> u32 {
        self.sample_rate
    }

    fn legnth(&self) -> u32 { unimplemented!() }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        let first = self.first_frame();
        target.resample(first.as_slice()).await;

        let mut frames : Vec<Vec<f32>> = Vec::new();
    
        loop {
            //decode further
            if let Ok(n) = self.decoder.next_frame() 
            {
                let curr_samples : Vec<f32> 
                    = n.data.iter()
                        .map(|x| *x as f32 / 32768.0)
                        .collect();
                
                target.resample(&mut curr_samples.as_slice()).await;
            }
            //nothing to decode
            else 
            {
                break;
            }
        }

    }
}

impl Tagged for Mp3Reader {
    fn tags(&self) -> Tags {
        self.tags.clone()
    }
}

pub type OpusReader<'a> = opusfile::Opusfile<'a>;

#[async_trait]
impl AudioProducer for OpusReader<'_> {
    fn open(file_name: &str) -> Option<Self> {
        opusfile::Opusfile::open(file_name).ok()
    }

    fn native_samplerate(&self) -> u32 { 48000 }

    fn legnth(&self) -> u32 { unimplemented!() }

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

impl Tagged for OpusReader<'_> {
    fn tags(&self) -> Tags {
        match opusfile::Opusfile::tags(self) {
            Some(tags) => Tags {
                artist: tags.get_tag("artist").join(", ").to_owned(),
                album: tags.get_tag("album").join(" ").to_owned(),
                title: tags.get_tag("title").join(" ").to_owned()
            },
            None => Tags::empty()
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

    fn legnth(&self) -> u32 { unimplemented!() }

    async fn read <T: ReaderTarget<f32>> (&mut self, mut target: Resampler<T>) {
        let mut blocks = self.blocks();
        let mut buffer = Vec::new();

        while let Ok(Some(chunk)) = blocks.read_next_or_eof(buffer) {
            buffer = chunk.into_buffer();
            target.resample(& buffer.iter().map(|x| *x as f32 / 32768.0).collect::<Vec<_>>()).await;
        }
    }
}

impl Tagged for FlacReader {
    fn tags(&self) -> Tags {
        let artists : Vec<&str> = self.get_tag("artist").collect();
        let album : Vec<&str> = self.get_tag("album").collect();
        let title : Vec<&str> = self.get_tag("title").collect();

        Tags {
            artist: artists.join(", ").to_owned(),
            album: album.join(" ").to_owned(),
            title: title.join(" ").to_owned()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Tags {
    artist: String,
    album: String,
    title: String
}

impl Tags {
    pub fn empty() -> Tags {
        Tags {
            artist: String::from(""),
            album: String::from(""),
            title: String::from("")
        }
    }

    pub fn artist(&self) -> String { self.artist.clone() }
    pub fn album(&self) -> String { self.album.clone() }
    pub fn title(&self) -> String { self.title.clone() }
}

pub trait Tagged {
    fn tags (&self) -> Tags;
}