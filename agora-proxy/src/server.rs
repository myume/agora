use std::{net::SocketAddr, time::Duration};

use agora_http_parser::{HTTPParseError, HTTPStatusCode, Request, Response};
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
                    let _ = stream
                        .write_all(b"HTTP/1.1 408 Request Timeout\r\n\r\n")
                        .await;
                }
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
                let response = Response::new(HTTPStatusCode::RequestHeaderFieldsTooLarge);
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
                            // question: what if the header is just unterminated forever?
                            continue;
                        }
                        Err(e) => {
                            // invalid http request
                            error!("Couldn't parse request: {e}");
                            let response = Response::new(HTTPStatusCode::BadRequest);
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

        let mut response = Response::new(HTTPStatusCode::OK);
        response.body(b"Hello World");
        if let Err(e) = stream.write_all(&response.into_bytes()).await {
            error!("Failed to send response: {e}");
        };
    }
}
