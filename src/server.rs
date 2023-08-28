use std::error::Error;

use ffservice::video_server::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:2001".parse()?; // TODO: command line arg

    start_server(addr).await?;

    Ok(())
}
