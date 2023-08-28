use std::time::Duration;

use cucumber::{given, then, when, World};
use ffservice::video_server::start_server;
use tokio::time::sleep;

#[derive(Debug, Default, World)]
pub struct FFServiceWorld {}

#[given("VideoService is running")]
async fn given_videoservice_is_running(_world: &mut FFServiceWorld) {
    // TODO
    sleep(Duration::from_secs(1)).await;
}

#[when(regex = r"^a TranscodeRequest with (.*?) is received$")]
async fn when_a_transcoderequest_with_is_received(_world: &mut FFServiceWorld, filename: String) {
    // TODO
    sleep(Duration::from_secs(1)).await;
    print!("TranscodeRequest: {}", filename)
}

#[then("a downscaled mp4 is returned")]
async fn then_a_downscaled_mp4_is_returned(_world: &mut FFServiceWorld) {
    // TODO
    sleep(Duration::from_secs(1)).await;
}

#[tokio::main]
async fn main() {
    let addr = "0.0.0.0:2001".parse().unwrap();

    tokio::spawn(start_server(addr));

    FFServiceWorld::run("tests/features").await;
}
