use futures_core::Future;
use std::{
    io::{BufRead, BufReader, Seek, Write},
    net::SocketAddr,
    pin::Pin,
};
use tempfile::NamedTempFile;
use tokio_stream::StreamExt;

use tonic::{transport::Server, Request, Response, Status, Streaming};
use video::{video_service_server::VideoService, TranscodeRequest, TranscodeResponse};

use crate::thumbnailer;

use self::video::{video_service_server::VideoServiceServer, VideoMetadata};

pub mod video {
    tonic::include_proto!("video");
}

pub fn start_server(addr: SocketAddr) -> impl Future<Output = Result<(), tonic::transport::Error>> {
    let service = VideoServiceImpl::default();

    Server::builder()
        .add_service(VideoServiceServer::new(service))
        .serve(addr)
}

#[derive(Debug, Default)]
struct VideoServiceImpl {}

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
        let mut target_width: u32 = 0;
        let mut target_height: u32 = 0;

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
                target_width = tr.target_width as u32;
                target_height = tr.target_height as u32;
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

        // Extracting thumbnail
        let file_path = temp_file.path();
        let thumbnailer_result = thumbnailer::get_thumbnail(file_path, target_width, target_height)
            .map_err(|err| Status::invalid_argument(format!("failed to get thumbnail {err}")))?; // TODO no need to fail on this, transcoding might still work?

        // TODO Transcoding

        // Writing the response stream
        // TODO use transcoded file
        temp_file.rewind()?;
        let converted_file = temp_file;

        let output_stream = async_stream::try_stream! {
            yield TranscodeResponse {
                metadata: Some(VideoMetadata {
                    width: thumbnailer_result.width as i32,
                    height: thumbnailer_result.height as i32,
                    duration_seconds: thumbnailer_result.duration_seconds as i32,
                }),
                thumbnail: thumbnailer_result.thumbnail.to_vec(),
                transcoded_chunk: vec![],
            };

            let mut buffer = BufReader::with_capacity(CHUNK_SIZE, converted_file);
            loop {
                let read_bytes = match buffer.fill_buf() {
                    Ok(buf) => {
                        yield TranscodeResponse {
                            metadata: None,
                            thumbnail: vec![],
                            transcoded_chunk: buf.to_vec(),
                        };
                        buf.len()
                    },
                    Err(error) => panic!("failed to fill_buf while streaming response: {error}"),
                };

                if read_bytes == 0 {
                    break; //EOF
                }

                buffer.consume(read_bytes);
            }
        };

        // TODO report stats properly
        eprintln!("read_bytes={}", size);

        Ok(Response::new(Box::pin(output_stream)))
    }
}
