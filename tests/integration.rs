use cucumber::{given, then, when, writer, World};
use ffservice::{
    video_client::{
        video::{video_service_client::VideoServiceClient, VideoMetadata},
        VideoClient,
    },
    video_server::start_server,
};

#[derive(Debug, Default, World)]
struct FFServiceWorld {
    last_metadata: Option<VideoMetadata>,
}

#[given("VideoService is running")]
async fn given_videoservice_is_running(_world: &mut FFServiceWorld) {
    // TODO eh... ping it?
}

#[when(regex = r"^a TranscodeRequest with (.*?) is received$")]
async fn when_a_transcoderequest_with_is_received(world: &mut FFServiceWorld, filename: String) {
    let addr = "http://127.0.0.1:2001"; // TODO: externalize
    let output_file = "target/converted.mp4"; // TODO: use temp file

    let mut client = VideoClient::new(VideoServiceClient::connect(addr).await.unwrap());
    let metadata = client
        .transcode_file(filename.as_str(), output_file, 1280, 720)
        .await
        .unwrap();
    world.last_metadata = Some(metadata);
}

#[then(regex = r"^the metadata has width=(.*?) height=(.*?) duration=(.*?)$")]
async fn then_the_metadata_has(world: &mut FFServiceWorld, width: i32, height: i32, duration: i32) {
    let m = world.last_metadata.as_ref().unwrap();
    assert_eq!(width, m.width);
    assert_eq!(height, m.height);
    assert_eq!(duration, m.duration_seconds);
}

#[tokio::main]
async fn main() {
    let addr = "0.0.0.0:2001".parse().unwrap(); // TODO: externalize

    tokio::spawn(start_server(addr));

    FFServiceWorld::cucumber()
        .with_writer(writer::Libtest::or_basic())
        .run("tests/features")
        .await;
}
