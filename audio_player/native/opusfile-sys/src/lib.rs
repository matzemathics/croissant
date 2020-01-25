#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
//use libc::*;
include!("bindings.rs");

#[cfg(test)]
mod tests {
    #[test]
    fn op_test_file_succeeds () {
        use std::path::Path;
        use std::ffi::CString;

        let mut i :i32 = 0;
        let out_dir = std::env::current_dir().expect("could not get out dir");
        let out_dir = Path::new(&out_dir);
        let static_test_path = out_dir.join("test.opus");
        println!("Testing opus file: {:?}", static_test_path);
        unsafe {
            crate::op_test_file(
                CString::new(static_test_path.to_str().unwrap()).unwrap().into_raw(), 
                &mut i);
        }
        assert_eq!(i,0);
    }
}
