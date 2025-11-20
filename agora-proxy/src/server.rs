use std::net::SocketAddr;

use agora_http_parser::{HTTPParseError, Request};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
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
            let (stream, addr) = listener.accept().await?;

            tokio::spawn(async move {
                Self::process(stream, addr).await;
            });
        }
    }

    async fn process(mut stream: TcpStream, addr: SocketAddr) {
        debug!("Connection Accepted: {addr}");

        let mut buf = [0; MAX_BUF_SIZE];
        let mut bytes_read = 0;
        let request = loop {
            if bytes_read >= buf.len() {
                // request header is too big
                if let Err(e) = stream
                    .write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n")
                    .await
                {
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
                            if let Err(e) =
                                stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await
                            {
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

        if let Err(e) = stream
            .write_all(b"HTTP/1.1 200 OK\r\n\r\nHello World")
            .await
        {
            error!("Failed to send response: {e}");
        };
    }
}
