extern crate ffmpeg_next as ffmpeg;
extern crate image;

use ffmpeg::{codec, format, format::context::*, media::Type, software, util::frame::Video};

fn main() -> Result<(), ffmpeg::Error> {
    ffmpeg::init().unwrap();

    let file_name = "samples/SampleVideo_1280x720_1mb.mp4";
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

    // Attempt to seek to 10% of the file
    // Note that this does not always work (keyframes and stuff), so we give an upper bound to the seek, and decode the frames until we hit our timestamp
    let seek_ts = input.start_time() + (input.duration() as f64 * 0.1) as i64;
    context.seek(seek_ts, 0..seek_ts)?;

    let packets = context
        .packets()
        .filter(|(stream, _)| stream.index() == video_stream_index)
        .map(|(_, packet)| packet);

    for packet in packets {
        decoder.send_packet(&packet)?;
        let dts = packet.dts().unwrap_or(i64::MAX);
        let mut decoded = Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() && dts >= seek_ts {
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
        println!("best_stream: {}", stream.index());
        println!("time_base: {}", stream.time_base());
        println!("start_time: {}", stream.start_time());
        println!("duration: {}", stream.duration());
    }
}
