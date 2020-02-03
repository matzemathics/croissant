
extern crate opusfile_sys;

use enum_primitive::*;

use std::path::Path;
use std::result::Result;
use std::ffi::CString;


enum_from_primitive! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[repr(i32)]
    pub enum Error {
        // A request did not succeed.
        OpFalse = (-1),
        
        //  
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


pub struct Opusfile (*mut opusfile_sys::OggOpusFile);
impl Opusfile {
    pub fn open<P: AsRef<Path>> (filename: P) -> Result<Opusfile, Error> {
        let path = CString::new(filename.as_ref().to_str().unwrap()).unwrap().into_raw();
        let mut error :i32 = 0;
        let handle = unsafe { opusfile_sys::op_open_file(path, &mut error) };

        if error != 0 {
            return Err(Error::from_i32(error).unwrap());
        }

        Ok(Opusfile(handle))
    }

    pub fn read_stereo (&mut self, target: &mut [f32]) -> Result<usize, Error> {
        let res = unsafe { opusfile_sys::op_read_float_stereo(self.0, target.as_mut_ptr(), target.len() as i32) };

        if res < 0 {
            Err(Error::from_i32(res).unwrap())
        } else {
            Ok(res as usize)
        }
    }
}

unsafe impl Send for Opusfile {}

impl Drop for Opusfile {
    fn drop(&mut self) {
        unsafe { opusfile_sys::op_free(self.0); }
    }
}