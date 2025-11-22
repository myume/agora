use std::{net::SocketAddr, time::Duration};

use agora_http_parser::{HTTPParseError, HTTPVersion, Request, Response};
use http::StatusCode;
use regex::Regex;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tracing::{debug, error, info};

const MAX_BUF_SIZE: usize = 4096 * 2;

pub struct Server {
    config: ServerConfig,
}

#[derive(Debug, Clone)]
pub struct ProxyEntry {
    pub addr: String,
}

#[derive(Debug, Default, Clone)]
pub struct ServerConfig {
    pub reverse_proxy_mapping: Vec<(Regex, ProxyEntry)>,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn listen(&self, address: &str) -> io::Result<()> {
        let listener = TcpListener::bind(address).await?;
        info!("Listening on {}", address);
        loop {
            let (mut stream, addr) = listener.accept().await?;

            let config = self.config.clone();
            tokio::spawn(async move {
                let result = timeout(
                    Duration::from_secs(30),
                    Self::process(&mut stream, addr, config),
                )
                .await;

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

    async fn process(client_stream: &mut TcpStream, addr: SocketAddr, config: ServerConfig) {
        debug!("Connection Accepted: {addr}");

        let mut buf = [0; MAX_BUF_SIZE];
        let mut bytes_read = 0;
        let request = loop {
            if bytes_read >= buf.len() {
                // request header is too big
                let mut response = Response::new(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
                response.header("Connection", "close");
                if let Err(e) = client_stream.write_all(&response.into_bytes()).await {
                    error!("Failed to send response: {e}");
                };
                return;
            }

            match client_stream.read(&mut buf[bytes_read..]).await {
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
                            if let Err(e) = client_stream.write_all(&response.into_bytes()).await {
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
            if let Err(e) = client_stream.write_all(&response.into_bytes()).await {
                error!("Failed to send response: {e}");
            };
            return;
        }

        // could be a performance issue iterating through lots of mappings
        let mut proxied_request = false;
        for (re, entry) in config.reverse_proxy_mapping {
            if re.is_match(request.path) {
                debug!("Proxing request to {}", entry.addr);
                proxied_request = true;

                let Ok(mut server_stream) = TcpStream::connect(&entry.addr).await else {
                    error!("Failed to establish TCP connection with {}", entry.addr);
                    let mut response = Response::new(StatusCode::BAD_GATEWAY);
                    response.header("Connection", "close");
                    if let Err(e) = client_stream.write_all(&response.into_bytes()).await {
                        error!("Failed to send response: {e}");
                    };
                    return;
                };

                // For now, assume that the full request fits into our buffer.
                // We will need to amend this assumption later, once we get the proxy working.
                if let Err(e) = server_stream.write_all(&buf[..bytes_read]).await {
                    error!("Failed to forward request to {}: {e}", entry.addr);
                    let mut response = Response::new(StatusCode::BAD_GATEWAY);
                    response.header("Connection", "close");
                    if let Err(e) = server_stream.write_all(&response.into_bytes()).await {
                        error!("Failed to send response: {e}");
                    };
                }

                // Notice that if multiple mappings match the same path,
                // the first one in the array will be chosen.
                break;
            }
        }

        if !proxied_request {
            let mut response = Response::new(StatusCode::NOT_FOUND);
            response.header("Connection", "close");
            response.body(b"Not Found");
            if let Err(e) = client_stream.write_all(&response.into_bytes()).await {
                error!("Failed to send response: {e}");
            };
        }
    }
}
