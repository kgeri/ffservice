extern crate ffmpeg_next as ffmpeg;

use std::error::Error;

use ffmpeg::{
    codec, decoder, encoder, format, format::context::Output, frame, media, picture, Dictionary,
    Packet, Rational,
};

fn main() -> Result<(), Box<dyn Error>> {
    ffmpeg::init()?; // TODO do we need this?

    let target_width = 1280;
    let target_height = 720;

    let input = format::input(&"samples/Tractor_500kbps_x265.mp4")?;
    let mut output = format::output(&"target/transcoded.mp4")?;

    let input_stream = input
        .streams()
        .best(media::Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?;

    let mut output_stream = output.add_stream(encoder::find(codec::Id::H264))?;

    let mut transcoder = VideoTranscoder::new(
        &input_stream,
        &mut output_stream,
        target_width,
        target_height,
    )?;

    output.set_metadata(input.metadata().to_owned()); // Copying metadata
    output.write_header()?; // Writing output header

    // Transcoding
    let mut ictx = input;
    for (stream, packet) in ictx.packets() {
        if stream.index() != transcoder.stream_idx {
            continue; // TODO transcode the other streams as well
        }

        // WTF: packet.rescale_ts(stream.time_base(), transcoder.decoder.time_base());
        transcoder.transcode_packet(&packet, &mut output)?;
    }

    // Flush encoders and decoders
    // TODO let ost_time_base = ost_time_bases[*ost_index];
    transcoder.eof(&mut output)?;

    output.write_trailer()?;

    Ok(())
}

trait Transcoder {
    fn transcode_packet(
        &mut self,
        packet: &Packet,
        output: &mut Output,
    ) -> Result<(), Box<dyn Error>>;

    fn eof(&mut self, output: &mut Output) -> Result<(), Box<dyn Error>>;
}

struct VideoTranscoder {
    stream_idx: usize,
    time_base: Rational,
    decoder: decoder::Video,
    encoder: encoder::Video,
}

impl Transcoder for VideoTranscoder {
    fn transcode_packet(
        &mut self,
        packet: &Packet,
        output: &mut Output,
    ) -> Result<(), Box<dyn Error>> {
        self.decoder.send_packet(packet)?;
        self.receive_and_process_decoded_frames(output)?;
        Ok(())
    }

    fn eof(&mut self, output: &mut Output) -> Result<(), Box<dyn Error>> {
        self.decoder.send_eof()?;
        self.receive_and_process_decoded_frames(output)?;
        self.encoder.send_eof()?;
        self.receive_and_process_encoded_packets(output)?;
        Ok(())
    }
}

impl VideoTranscoder {
    fn new(
        input_stream: &format::stream::Stream,
        output_stream: &mut format::stream::StreamMut,
        target_width: u32,
        target_height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let x264_opts = Dictionary::new();

        let stream_idx = input_stream.index();
        let decoder = codec::Context::from_parameters(input_stream.parameters())?
            .decoder()
            .video()?;

        let mut encoder_ctx = codec::Context::from_parameters(output_stream.parameters())?
            .encoder()
            .video()?;
        encoder_ctx.set_width(target_width);
        encoder_ctx.set_height(target_height);
        encoder_ctx.set_aspect_ratio(decoder.aspect_ratio());
        encoder_ctx.set_format(decoder.format());
        encoder_ctx.set_frame_rate(decoder.frame_rate());
        encoder_ctx.set_time_base(decoder.time_base());
        encoder_ctx.set_flags(codec::Flags::GLOBAL_HEADER);

        let encoder = encoder_ctx.open_with(x264_opts)?;
        output_stream.set_parameters(&encoder);

        Ok(Self {
            stream_idx,
            time_base: output_stream.time_base(),
            decoder,
            encoder,
        })
    }

    fn receive_and_process_decoded_frames(
        &mut self,
        output: &mut Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut frame = frame::Video::empty();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            let timestamp = frame.timestamp();
            frame.set_pts(timestamp);
            frame.set_kind(picture::Type::None);
            self.encoder.send_frame(&frame)?;
            self.receive_and_process_encoded_packets(output)?;
        }
        Ok(())
    }

    fn receive_and_process_encoded_packets(
        &mut self,
        output: &mut Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut encoded = Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(self.stream_idx);
            encoded.rescale_ts(self.decoder.time_base(), self.time_base);
            encoded.write_interleaved(output)?;
        }
        Ok(())
    }
}
