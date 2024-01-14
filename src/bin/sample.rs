use anyhow::Result;
use lib_ffmpeg::transcode;

fn main() -> Result<()> {
    transcode(
        "samples/Tractor_500kbps_x265.mp4",
        "target/transcoded.mp4",
        1280,
        720,
    );

    Ok(())
}
