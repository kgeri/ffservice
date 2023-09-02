extern crate ffmpeg_next as ffmpeg;
extern crate image;

use std::{error::Error, path::Path};

use ffmpeg::{codec, format, media, software, util::frame::Video, Rational};

pub struct ThumbnailerResult {
    pub thumbnail: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub duration_seconds: u64,
}

pub fn get_thumbnail(
    file_path: &Path,
    target_width: u32,
    target_height: u32,
) -> Result<ThumbnailerResult, Box<dyn Error>> {
    let mut input = format::input(&file_path)?;

    let stream = input
        .streams()
        .best(media::Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let video_stream_index = stream.index();
    let time_base = stream.time_base();
    let duration = stream.duration();

    let context_decoder = codec::Context::from_parameters(stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;
    let mut scaler = software::scaling::context::Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        format::Pixel::RGB24,
        target_width,
        target_height,
        software::scaling::Flags::BILINEAR,
    )?;

    // Attempt to seek to 10% of the file
    // Note that this does not always work (keyframes and stuff), so we give an upper bound to the seek, and decode the frames until we hit our timestamp
    let seek_ts = stream.start_time() + (stream.duration() as f64 * 0.1) as i64;
    input.seek(seek_ts, 0..seek_ts)?;

    let packets = input
        .packets()
        .filter(|(stream, _)| stream.index() == video_stream_index)
        .map(|(_, packet)| packet);

    let mut frame = Video::empty();
    for packet in packets {
        decoder.send_packet(&packet)?;
        let dts = packet.dts().unwrap_or(i64::MAX);
        let mut decoded = Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() && dts >= seek_ts {
            scaler.run(&decoded, &mut frame)?;
            break;
        }
    }
    decoder.send_eof()?;

    let thumbnail = frame.data(0).to_vec(); // TODO avoid the copy somehow (without also exposing ffmpeg types)?
    Ok(ThumbnailerResult {
        thumbnail,
        width: decoder.width(),
        height: decoder.height(),
        duration_seconds: get_duration(time_base, duration),
    })
}

fn get_duration(time_base: Rational, duration: i64) -> u64 {
    let nom: u64 = (duration as u64) * (time_base.numerator() as u64);
    nom / (time_base.denominator() as u64)
}
