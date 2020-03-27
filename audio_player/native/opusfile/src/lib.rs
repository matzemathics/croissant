
//+---------------------------------------------------------------------+
//| lib.rs - bietet eine Schnittstelle für die libopusfile Bibiliothek, |
//|     die das Dekodieren von Opus-Dateien ermöglicht. Die rust-An-    |
//|     bindung dieser Bibiliothek befindet sich im crate opusfile-sys. |
//|     Diese Datei vereinfacht den Zugriff auf bestimmte Bibiliotheks- |
//|     funktionen.                                                     |
//| siehe auch https://github.com/xiph/opusfile                         |
//+---------------------------------------------------------------------+

// automatisch erzeugte rust-Anbindung durch rust-bindgen
extern crate opusfile_sys;

use enum_primitive::*;

use std::{
    path::Path,
    result::Result,
    convert::TryInto,
    ffi::{CString},
    slice::from_raw_parts,
    str::from_utf8_unchecked
};

//+----------------------------------------------------------------------------------------
//| enum Error 
//|     - Rückgabecodes der libopusfile Funktionen
//|     - Erklärungen aus der Dokumentation übernommen
//|       (siehe https://www.opus-codec.org/docs/opusfile_api-0.7/group__error__codes.html)

enum_from_primitive! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[repr(i32)]
    pub enum Error {
        // A request did not succeed.
        OpFalse = (-1),
        
        // End of File
        OpEof = (-2),
        
        // There was a hole in the page sequence numbers (e.g., a page was corrupt or missing). 
        OpHole = (-3),

        // An underlying read, seek, or tell operation failed when it should have succeeded. 
        OpEread = (-128),

        // A NULL pointer was passed where one was unexpected, or an internal memory allocation failed, or an internal library error was encountered. 
        OpEfault = (-129),

        // The stream used a feature that is not implemented, such as an unsupported channel family. 
        OpEimpl = (-130),

        // One or more parameters to a function were invalid. 
        OpEinval = (-131),

        // A purported Ogg Opus stream did not begin with an Ogg page, a purported header packet did not start with one of the required strings, "OpusHead" or "OpusTags", or a link in a chained file was encountered that did not contain any logical Opus streams. 
        OpEnotformat = (-132),

        // A required header packet was not properly formatted, contained illegal values, or was missing altogether. 
        OpEbadheader = (-133),

        // The ID header contained an unrecognized version number. 
        OpEversion = (-134),

        //  
        OpEnotaudio = (-135),

        // An audio packet failed to decode properly. 
        OpEbadpacket = (-136),

        // We failed to find data we had seen before, or the bitstream structure was sufficiently malformed that seeking to the target destination was impossible. 
        OpEbadlink = (-137),

        // An operation that requires seeking was requested on an unseekable stream. 
        OpEnoseek = (-138),

        // The first or last granule position of a link failed basic validity checks. 
        OpEbadtimestamp = (-139),
    }
}

//+------------------------------------------------------------------------------
//| struct Opusfile<'a>
//|     - einfaches Öffnen und Dekodieren von Opus-Dateien

pub struct Opusfile<'a> (&'a mut opusfile_sys::OggOpusFile);
impl Opusfile<'_> {

    // öffnet eine Datei
    pub fn open<'a, P: AsRef<Path>> (filename: P) -> Result<Opusfile<'a>, Error> 
    {
        let path = CString::new(filename.as_ref().to_str().unwrap()).unwrap().into_raw();
        let mut error :i32 = 0;
        let handle = unsafe { opusfile_sys::op_open_file(path, &mut error) };

        if error != 0 {
            return Err(Error::from_i32(error).unwrap());
        }

        Ok(Opusfile(unsafe { handle.as_mut().unwrap() }))
    }

    // liest die Opus-Datei in einen Buffer (target) und 
    // gibt bei erfolgreicher Dekodierung deren Länge zurück
    pub fn read_stereo (&mut self, target: &mut [f32]) -> Result<usize, Error> {
        let res = unsafe { 
            opusfile_sys::op_read_float_stereo(
                self.0, 
                target.as_mut_ptr(), 
                target.len() as i32) 
        };

        if res < 0 {
            Err(Error::from_i32(res).unwrap())
        } else {
            Ok(res as usize)
        }
    }

    // Liest alle Metadaten aus der Datei
    pub fn tags<'a> (this: &Opusfile<'a>) -> Option<Tags<'a>> {
        unsafe {
            opusfile_sys::op_tags(this.0, 0)
                .as_ref().map(|t| Tags::new(t)) 
        }
    }
}

// markiert die Struktur Opusfile als Threadsicher
unsafe impl Send for Opusfile<'_> {}

// bei Freigabe (drop) der Struktur Opusfile
// wird die Datei korrekt geschlossen
impl Drop for Opusfile<'_> {
    fn drop(&mut self) {
        unsafe { opusfile_sys::op_free(self.0); }
    }
}

//+--------------------------------------------------------
//| struct Tags
//|     - im Falle von Opushandelt es sich um Metadaten im
//|       Vorbis-Format. Die Information über Künstler,
//|       Titel, usw. werden als Name-Wert-Paare gespeichert.

pub struct Tags<'a> (Vec<(&'a str, &'a str)>);

impl<'a> Tags<'a> {
    // Dekodierung der Tags
    fn new (tags: &'a opusfile_sys::OpusTags) -> Tags<'a> 
    {
        let num_tags = tags.comments.try_into().unwrap();
        let raw_tags = unsafe { 
            let lengths = from_raw_parts(tags.comment_lengths, num_tags);
            let comments = from_raw_parts(tags.user_comments, num_tags);
            comments.iter().zip(lengths)
        };
        
        let mut res = Vec::new();
        for (tag, length) in raw_tags {
            let s = unsafe {
                from_utf8_unchecked(from_raw_parts(*tag as *const u8, *length as usize))
            };
            let fields : Vec<&str> = s.split('=').collect();
            assert_eq!(fields.len(), 2);
            let label = fields[0];
            let content = fields[1];
            res.push((label, content));
        }
        Tags(res)
    }

    // eine einzelne Information erfragen
    pub fn get_tag(&self, tag: &str) -> Vec<&'a str>
    {
        let mut res = Vec::new();
        for &(t, v) in self.0.as_slice() {
            if t.to_uppercase() == tag.to_uppercase() {
                res.push(v);
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn op_test_file_succeeds () {
        use std::path::Path;

        let out_dir = std::env::current_dir().expect("could not get out dir");
        let out_dir = Path::new(&out_dir);
        let static_test_path = out_dir.join("test.opus");
        println!("Testing opus file: {:?}", static_test_path);
        
        let f = crate::Opusfile::open(static_test_path).unwrap();
        let t = crate::Opusfile::tags(&f).unwrap();

        assert_eq!(t.get_tag("title"), vec!["Illustrated Man"]);
    }
}
