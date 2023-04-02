extern crate ffmpeg_next as ffmpeg;
extern crate image;

use ffmpeg::{codec, format, format::context::*, media::Type, software, util::frame::Video};

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

    let packets = context
        .packets()
        .filter(|(stream, _)| stream.index() == video_stream_index)
        .map(|(_, packet)| packet);

    for packet in packets {
        decoder.send_packet(&packet)?;
        let mut decoded = Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() {
            let mut frame = Video::empty();
            scaler.run(&decoded, &mut frame)?;

            image::save_buffer(
                "frame.jpg",
                frame.data(0),
                frame.width(),
                frame.height(),
                image::ColorType::Rgb8,
            )
            .unwrap(); // TODO better error reporting
            break;
        }
    }
    decoder.send_eof()?;
    // process_decoded_frames(&mut decoder);

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
