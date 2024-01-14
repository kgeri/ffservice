#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CString;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub fn transcode(
    input_file_name: &str,
    output_file_name: &str,
    target_width: i32,
    target_height: i32,
) {
    let if_cstr = CString::new(input_file_name).unwrap();
    let of_cstr = CString::new(output_file_name).unwrap();
    unsafe {
        ffmpeg_transcode(
            if_cstr.as_ptr(),
            of_cstr.as_ptr(),
            target_width,
            target_height,
            0,
        );
    }
}
