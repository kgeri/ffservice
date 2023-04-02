extern crate ffmpeg_next as ffmpeg;

use ffmpeg::{codec, format, format::context::*, media::Type, software, util::frame::Video};
use std::{fs::File, io::Write};

fn main() -> Result<(), ffmpeg::Error> {
    ffmpeg::init().unwrap();

    let file_name = "samples/Tractor_500kbps_x265.mp4";
    match format::input(&file_name) {
        Ok(mut input) => {
            print_metadata(&input);
            thumbnail(&mut input)?;
        }
        Err(_) => todo!(),
    }
    Ok(())
}

fn thumbnail(context: &mut Input) -> Result<(), ffmpeg::Error> {
    let input = context
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let video_stream_index = input.index();

    let context_decoder = codec::Context::from_parameters(input.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    let mut scaler = software::scaling::context::Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        format::Pixel::RGB24,
        800,
        600,
        software::scaling::Flags::BILINEAR,
    )?;

    let mut process_decoded_frames =
        |decoder: &mut ffmpeg::decoder::Video| -> Result<(), ffmpeg::Error> {
            let mut decoded = Video::empty();
            if decoder.receive_frame(&mut decoded).is_ok() {
                let mut rgb_frame = Video::empty();
                scaler.run(&decoded, &mut rgb_frame)?;
                save_file(&rgb_frame, 0).unwrap();
            }
            Ok(())
        };

    for (stream, packet) in context.packets() {
        if stream.index() == video_stream_index {
            decoder.send_packet(&packet)?;
            process_decoded_frames(&mut decoder)?;
        }
    }
    decoder.send_eof()?;
    // process_decoded_frames(&mut decoder);

    Ok(())
}

fn save_file(frame: &Video, index: usize) -> std::result::Result<(), std::io::Error> {
    let mut file = File::create(format!("frame{}.ppm", index))?;
    file.write_all(format!("P6\n{} {}\n255\n", frame.width(), frame.height()).as_bytes())?;
    file.write_all(frame.data(0))?;
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
