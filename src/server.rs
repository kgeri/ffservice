mod video_server;

use std::error::Error;

use tonic::transport::Server;
use video_server::{video::video_service_server::VideoServiceServer, VideoServiceImpl};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:2001".parse()?; // TODO: command line arg
    let service = VideoServiceImpl::default();

    Server::builder()
        .add_service(VideoServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
