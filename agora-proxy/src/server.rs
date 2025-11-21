use std::{net::SocketAddr, time::Duration};

use agora_http_parser::{HTTPParseError, HTTPVersion, Request, Response};
use http::StatusCode;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tracing::{debug, error, info};

pub struct Server {
    address: String,
}

const MAX_BUF_SIZE: usize = 4096 * 2;

impl Server {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    pub async fn listen(&self) -> io::Result<()> {
        let listener = TcpListener::bind(&self.address).await?;
        info!("Listening on {}", self.address);
        loop {
            let (mut stream, addr) = listener.accept().await?;

            tokio::spawn(async move {
                let result =
                    timeout(Duration::from_secs(30), Self::process(&mut stream, addr)).await;

                if result.is_err() {
                    error!("Connection timed out: {addr}");
                    let mut response = Response::new(StatusCode::REQUEST_TIMEOUT);
                    response.header("Connection", "close");
                    if let Err(e) = stream.write_all(&response.into_bytes()).await {
                        error!("Failed to send response: {e}");
                    }
                }

                stream.shutdown().await
            });
        }
    }

    async fn process(stream: &mut TcpStream, addr: SocketAddr) {
        debug!("Connection Accepted: {addr}");

        let mut buf = [0; MAX_BUF_SIZE];
        let mut bytes_read = 0;
        let request = loop {
            if bytes_read >= buf.len() {
                // request header is too big
                let mut response = Response::new(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
                response.header("Connection", "close");
                if let Err(e) = stream.write_all(&response.into_bytes()).await {
                    error!("Failed to send response: {e}");
                };
                return;
            }

            match stream.read(&mut buf[bytes_read..]).await {
                Ok(0) => {
                    // connection closed
                    return;
                }
                Ok(n) => {
                    bytes_read += n;
                    match Request::parse(&buf[..bytes_read]) {
                        Ok(request) => break request,
                        Err(HTTPParseError::UnterminatedHeader) => {
                            continue;
                        }
                        Err(e) => {
                            // invalid http request
                            error!("Couldn't parse request: {e}");
                            let mut response = Response::new(StatusCode::BAD_REQUEST);
                            response.header("Connection", "close");
                            if let Err(e) = stream.write_all(&response.into_bytes()).await {
                                error!("Failed to send response: {e}");
                            };
                            return;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read from socket: {e}");
                    return;
                }
            }
        };

        debug!("{request}");

        if request.version != HTTPVersion::HTTP1_1 {
            let mut response = Response::new(StatusCode::HTTP_VERSION_NOT_SUPPORTED);
            response.header("Connection", "close");
            return;
        }

        let mut response = Response::new(StatusCode::OK);
        response.body(b"Hello World");
        if let Err(e) = stream.write_all(&response.into_bytes()).await {
            error!("Failed to send response: {e}");
        };
    }
}
