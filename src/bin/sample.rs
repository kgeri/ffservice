use anyhow::Result;
use lib_ffmpeg::open_video;

fn main() -> Result<()> {
    open_video("samples/Tractor_500kbps_x2652.mp4");

    Ok(())
}
