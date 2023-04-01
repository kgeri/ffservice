extern crate ffmpeg_next as ffmpeg;

use ffmpeg::format::context::*;

fn main() -> Result<(), ffmpeg::Error> {
    ffmpeg::init().unwrap();

    let file_name = "samples/Tractor_500kbps_x265.mp4";
    match ffmpeg::format::input(&file_name) {
        Ok(context) => print_metadata(&context),
        Err(error) => println!("Failed to open file: {}", error),
    }
    Ok(())
}

fn print_metadata(context: &Input) {
    for (k, v) in context.metadata().iter() {
        println!("{}: {}", k, v);
    }

    if let Some(stream) = context.streams().best(ffmpeg::media::Type::Video) {
        println!("Best video stream index: {}", stream.index());
    }
}
