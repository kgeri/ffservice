mod video_client;

use std::error::Error;

use video_client::video::video_service_client::VideoServiceClient;
use video_client::VideoClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO: command line args
    let addr = "http://127.0.0.1:2001";
    let input_file = "samples/Tractor_500kbps_x265.mp4";
    let output_file = "target/converted.mp4";
    let target_width = 1280;
    let target_height = 720;

    let mut client = VideoClient::new(VideoServiceClient::connect(addr).await?);
    let metadata = client.transcode_file(input_file, output_file, target_width, target_height).await?;

    println!("Transcoded successfully: {metadata:?}, outputFile={output_file}");
    Ok(())
}
