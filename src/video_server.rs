use std::{
    io::{Read, Seek, Write},
    pin::Pin,
};
use tempfile::NamedTempFile;
use tokio_stream::StreamExt;

use tonic::{Request, Response, Status, Streaming};
use video::{video_service_server::VideoService, TranscodeRequest, TranscodeResponse};

use self::video::VideoMetadata;

pub mod video {
    tonic::include_proto!("video");
}

#[derive(Debug, Default)]
pub struct VideoServiceImpl {}

const CHUNK_SIZE: usize = 1024 * 1024;

#[tonic::async_trait]
impl VideoService for VideoServiceImpl {
    type TranscodeStream = Pin<
        // someday, I'll understand what the hell all this is...
        Box<dyn futures_core::Stream<Item = Result<TranscodeResponse, Status>> + Send + 'static>,
    >;

    async fn transcode(
        &self,
        request: Request<Streaming<TranscodeRequest>>,
    ) -> Result<Response<Self::TranscodeStream>, Status> {
        let mut size: i32 = 0;
        let mut target_width = 0;
        let mut target_height = 0;

        // Reading the request stream and saving it into a temporary file
        let mut stream = request.into_inner();
        let mut temp_file_option: Option<NamedTempFile> = None;

        while let Some(result) = stream.next().await {
            let tr = result?;

            if !tr.extension.is_empty() {
                let extension = tr.extension.as_str();
                let temp_file = tempfile::Builder::new().suffix(extension).tempfile()?;
                temp_file_option = Some(temp_file);
            }
            if tr.target_width > 0 && tr.target_height > 0 {
                target_width = tr.target_width;
                target_height = tr.target_height;
            }

            let temp_file = temp_file_option.as_mut().ok_or(Status::invalid_argument(
                "extension must be specified in the first TranscodeRequest",
            ))?;
            temp_file.write_all(&tr.request_chunk)?;

            size += tr.request_chunk.len() as i32;
        }

        let mut temp_file = temp_file_option.ok_or(Status::invalid_argument(
            "extension must be specified in the first TranscodeRequest",
        ))?;

        // TODO Extracting metadata
        let duration_seconds = 0;

        // TODO Transcoding

        // Writing the response stream
        // TODO use transcoded file
        temp_file.rewind()?;
        let mut converted_file = temp_file;

        let mut buf: Vec<u8> = Vec::with_capacity(CHUNK_SIZE);

        let output_stream = async_stream::try_stream! {
            yield TranscodeResponse {
                metadata: Some(VideoMetadata {
                    width: target_width,
                    height: target_height,
                    duration_seconds: duration_seconds,
                }),
                thumbnail: vec![], // TODO Thumbnailing
                transcoded_chunk: vec![],
            };

            while let Ok(read_bytes) = converted_file.read_to_end(buf.as_mut()) {
                yield TranscodeResponse {
                    metadata: None,
                    thumbnail: vec![],
                    transcoded_chunk: buf[..read_bytes].to_vec(),
                };
            }
        };

        // TODO report stats properly
        eprintln!("read_bytes={}", size);

        Ok(Response::new(Box::pin(output_stream)))
    }
}
