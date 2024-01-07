use std::env;
use std::path::PathBuf;

fn main() {
    let lib_name = "ffmpeg";
    let libdir_path = PathBuf::from("src")
        .canonicalize()
        .expect("cannot canonicalize path");

    let headers_path = libdir_path.join("transcoder.h");
    let headers_path_str = headers_path.to_str().expect("path is not a valid string");

    println!("cargo:rustc-link-search={}", libdir_path.to_str().unwrap());
    println!("cargo:rustc-link-lib=asan"); // AddressSanitizer
    println!("cargo:rustc-link-lib={}", lib_name);
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avfilter");
    println!("cargo:rustc-link-lib=avformat");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rerun-if-changed=src");

    cc::Build::new()
        .file(libdir_path.join("transcoder.c"))
        .flag("-fsanitize=address") // AddressSanitizer
        .compile(lib_name);

    let bindings = bindgen::Builder::default()
        .header(headers_path_str)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("Couldn't write bindings!");
}
