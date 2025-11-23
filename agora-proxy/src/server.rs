use std::net::SocketAddr;

use agora_http_parser::{HTTPParseError, HTTPVersion, Request, Response};
use http::StatusCode;
use regex::Regex;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::{debug, error, info, warn};

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
                Self::process(&mut stream, addr, config).await;
                stream.shutdown().await
            });
        }
    }

    async fn process(client_stream: &mut TcpStream, addr: SocketAddr, config: ServerConfig) {
        debug!("Connection Accepted: {addr}");

        let mut buf = [0; MAX_BUF_SIZE];
        let request = match Self::read_request(client_stream, &mut buf).await {
            Ok(request) => request,
            Err(ref e) => {
                let mut response = match e.kind() {
                    // Failed to parse request
                    io::ErrorKind::UnexpectedEof => {
                        warn!("Couldn't parse request: stream closed prematurely");
                        Response::new(StatusCode::BAD_REQUEST)
                    }
                    io::ErrorKind::InvalidData => {
                        warn!("Couldn't parse request: Invalid Data");
                        Response::new(StatusCode::BAD_REQUEST)
                    }
                    // Request header too big
                    io::ErrorKind::OutOfMemory => {
                        warn!("Couldn't parse request: Header too large");
                        Response::new(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE)
                    }
                    // There was a problem reading related to the network
                    _ => {
                        // not much we can do to recover from this
                        error!("Failed to read request from {addr}: {e}");
                        return;
                    }
                };
                response.header("Connection", "close");
                Self::send_response(client_stream, response).await;
                return;
            }
        };

        debug!("{request}");

        if request.version != HTTPVersion::HTTP1_1 {
            let mut response = Response::new(StatusCode::HTTP_VERSION_NOT_SUPPORTED);
            response.header("Connection", "close");
            Self::send_response(client_stream, response).await;
            return;
        }

        // could be a performance issue iterating through lots of mappings
        let mut proxied_request = false;
        for (re, entry) in config.reverse_proxy_mapping {
            if re.is_match(&request.path) {
                debug!("Proxying request to {}", entry.addr);
                proxied_request = true;

                let Ok(mut server_stream) = TcpStream::connect(&entry.addr).await else {
                    error!("Failed to establish TCP connection with {}", entry.addr);
                    let mut response = Response::new(StatusCode::BAD_GATEWAY);
                    response.header("Connection", "close");
                    Self::send_response(client_stream, response).await;
                    return;
                };

                // For now, assume that the full request fits into our buffer.
                // We will need to amend this assumption later, once we get the proxy working.
                if let Err(e) = server_stream.write_all(&request.into_bytes()).await {
                    error!("Failed to forward request to {}: {e}", entry.addr);
                    let mut response = Response::new(StatusCode::BAD_GATEWAY);
                    response.header("Connection", "close");
                    Self::send_response(&mut server_stream, response).await;
                }

                loop {
                    match server_stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = client_stream.write_all(&buf[..n]).await {
                                error!("Failed to forward response to {}: {e}", addr);
                                let mut response = Response::new(StatusCode::BAD_GATEWAY);
                                response.header("Connection", "close");
                                Self::send_response(&mut server_stream, response).await;
                            }
                        }
                        Err(e) => {
                            error!("Failed to forward request to {}: {e}", addr);
                            let mut response = Response::new(StatusCode::BAD_GATEWAY);
                            response.header("Connection", "close");
                            Self::send_response(&mut server_stream, response).await;
                            return;
                        }
                    }
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
            Self::send_response(client_stream, response).await
        }
    }

    async fn send_response(stream: &mut TcpStream, response: Response<'static>) {
        if let Err(e) = stream.write_all(&response.into_bytes()).await {
            error!("Failed to send response: {e}");
        };
    }

    async fn read_request(
        stream: &mut TcpStream,
        buf: &mut [u8; MAX_BUF_SIZE],
    ) -> io::Result<Request> {
        let mut bytes_read = 0;
        loop {
            if bytes_read >= buf.len() {
                // request header is too big
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "Request Header too large",
                ));
            }

            match stream.read(&mut buf[bytes_read..]).await {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Couldn't parse request",
                    ));
                }
                Ok(n) => {
                    bytes_read += n;
                    match Request::parse(&buf[..bytes_read]) {
                        Ok(request) => break Ok(request),
                        Err(HTTPParseError::UnterminatedHeader) => {
                            continue;
                        }
                        Err(e) => {
                            // invalid http request
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("Couldn't parse request: {e}"),
                            ));
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}
