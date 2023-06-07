use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use tonic::transport::Channel;
use tonic::Request;
use video::video_service_client::VideoServiceClient;

use self::video::{TranscodeRequest, VideoMetadata};

pub mod video {
    tonic::include_proto!("video");
}

pub struct VideoClient {
    client: VideoServiceClient<Channel>,
}

const CHUNK_SIZE: usize = 1024 * 1024;

impl VideoClient {
    pub fn new(client: VideoServiceClient<Channel>) -> VideoClient {
        Self { client }
    }

    pub async fn transcode_file(
        &mut self,
        input_filename: &str,
        output_filename: &str,
        target_width: i32,
        target_height: i32,
    ) -> Result<VideoMetadata, Box<dyn Error>> {
        let extension = Path::new(input_filename)
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_string();
        let in_file = File::open(input_filename)?;

        let outbound = async_stream::stream! {
            yield TranscodeRequest {
                extension: extension,
                target_width: target_width,
                target_height: target_height,
                request_chunk: vec![]
            };

            let mut buffer = BufReader::with_capacity(CHUNK_SIZE, in_file);
            loop {
                let read_bytes = match buffer.fill_buf() {
                    Ok(buf) => {
                        yield TranscodeRequest {
                            extension: String::new(),
                            target_width: 0,
                            target_height: 0,
                            request_chunk: buf.to_vec()
                        };
                        buf.len()
                    },
                    Err(error) => panic!("error"), // TODO
                };

                if read_bytes == 0 {
                    break; // EOF
                }

                buffer.consume(read_bytes);
            }
        };

        let response = self.client.transcode(Request::new(outbound)).await?;
        let mut inbound = response.into_inner();

        let mut metadata: VideoMetadata = VideoMetadata { width: 0, height: 0, duration_seconds: 0 };
        let mut output_file = File::create(output_filename)?;
        let mut thumbnail_file = File::create(format!("{output_filename}.jpg"))?;
        while let Some(tr) = inbound.message().await? {
            if tr.metadata.is_some() {
                metadata = tr.metadata.unwrap();
            }
            if !tr.thumbnail.is_empty() {
                thumbnail_file.write_all(&tr.thumbnail)?;
            }
            if !tr.transcoded_chunk.is_empty() {
                output_file.write_all(&tr.transcoded_chunk)?;
            }
        }

        Ok(metadata)
    }
}
