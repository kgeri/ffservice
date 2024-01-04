#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CString;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub fn open_video(file_name: &str) {
    let file_name_cstr = CString::new(file_name).unwrap();
    unsafe {
        ffmpeg_open(file_name_cstr.as_ptr());
    }
}
