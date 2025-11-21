use std::{collections::HashMap, net::SocketAddr, time::Duration};

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
    addr: String,
}

#[derive(Debug, Default, Clone)]
pub struct ServerConfig {
    pub reverse_proxy_mapping: HashMap<Regex, ProxyEntry>,
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

    async fn process(stream: &mut TcpStream, addr: SocketAddr, config: ServerConfig) {
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
            if let Err(e) = stream.write_all(&response.into_bytes()).await {
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

                // Notice that if multiple mappings match the same path, the first one will be chosen
                // Ah but a hashmap's order is underterministic...
                break;
            }
        }

        if !proxied_request {
            let mut response = Response::new(StatusCode::NOT_FOUND);
            response.header("Connection", "close");
            response.body(b"Not Found");
            if let Err(e) = stream.write_all(&response.into_bytes()).await {
                error!("Failed to send response: {e}");
            };
        }
    }
}
