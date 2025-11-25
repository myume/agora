use std::net::SocketAddr;

use agora_http_parser::{HTTPVersion, Request, Response, is_terminated};
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
        let (request, remaining_body) = match Self::read_request(client_stream, &mut buf).await {
            Ok(request) => request,
            Err(ref e) => {
                let reason = match e.kind() {
                    // Failed to parse request
                    io::ErrorKind::UnexpectedEof => {
                        warn!("Couldn't parse request: Stream closed prematurely.");
                        StatusCode::BAD_REQUEST
                    }
                    io::ErrorKind::InvalidData => {
                        warn!("Couldn't parse request: Invalid Data.");
                        StatusCode::BAD_REQUEST
                    }
                    // Request header too big
                    io::ErrorKind::OutOfMemory => {
                        warn!("Couldn't parse request: Header too large.");
                        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
                    }
                    // There was a problem reading related to the network
                    _ => {
                        // not much we can do to recover from this
                        error!("Failed to read request from {addr}: {e}");
                        return;
                    }
                };
                Self::close_connection_with_reason(client_stream, reason).await;
                return;
            }
        };

        debug!("{request}");

        if request.version != HTTPVersion::HTTP1_1 {
            Self::close_connection_with_reason(
                client_stream,
                StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            )
            .await;
            return;
        }

        // could be a performance issue iterating through lots of mappings
        let mut proxied_request = false;
        for (re, entry) in config.reverse_proxy_mapping {
            if re.is_match(&request.path) {
                debug!("Proxying request to {}", entry.addr);
                proxied_request = true;

                let Ok(mut server_stream) = TcpStream::connect(&entry.addr).await else {
                    error!(
                        "Failed to establish TCP connection with server: {}",
                        entry.addr
                    );
                    Self::close_connection_with_reason(client_stream, StatusCode::BAD_GATEWAY)
                        .await;
                    return;
                };

                // For now, assume that the full request fits into our buffer.
                // We will need to amend this assumption later, once we get the proxy working.
                let mut request_bytes = request.into_bytes();
                request_bytes.extend(remaining_body);
                if let Err(e) = server_stream.write_all(&request_bytes).await {
                    error!("Failed to forward request to {}: {e}", entry.addr);
                    Self::close_connection_with_reason(client_stream, StatusCode::BAD_GATEWAY)
                        .await;
                    return;
                }

                loop {
                    match server_stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = client_stream.write_all(&buf[..n]).await {
                                error!("Failed to forward response to client {}: {e}", addr);
                                return;
                            }
                        }
                        Err(e) => {
                            error!("Failed to forward request to backend {}: {e}", addr);
                            Self::close_connection_with_reason(
                                client_stream,
                                StatusCode::BAD_GATEWAY,
                            )
                            .await;
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
            Self::close_connection_with_reason(client_stream, StatusCode::NOT_FOUND).await;
        }
    }

    async fn close_connection_with_reason(stream: &mut TcpStream, status_code: StatusCode) {
        let mut response = Response::new(status_code);
        response.header("Connection", "close");
        Self::send_response(stream, response).await;
    }

    async fn send_response(stream: &mut TcpStream, response: Response) {
        if let Err(e) = stream.write_all(&response.into_bytes()).await {
            error!("Failed to send response: {e}");
        };
    }

    async fn read_request<'buf>(
        stream: &mut TcpStream,
        buf: &'buf mut [u8; MAX_BUF_SIZE],
    ) -> io::Result<(Request, &'buf [u8])> {
        let mut total_bytes_read: usize = 0;
        let mut recent_bytes_read = 0;

        // We only scan the most recent bytes.
        // There could be a case where the terminator is split into 2 reads,
        // in that case we want to reread the last 4 bytes instead of just the most recently
        // appended bytes.
        // TLDR; we always read at least 4 bytes to ensure that we are able to find the terminator
        while !is_terminated(
            &buf[(total_bytes_read - recent_bytes_read).min(total_bytes_read.saturating_sub(4))
                ..total_bytes_read],
        ) {
            if total_bytes_read >= buf.len() {
                // request header is too big
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "Request Header too large",
                ));
            }

            match stream.read(&mut buf[total_bytes_read..]).await {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Couldn't parse request",
                    ));
                }
                Ok(n) => {
                    total_bytes_read += n;
                    recent_bytes_read = n;
                }
                Err(e) => return Err(e),
            }
        }

        Request::parse(&buf[..total_bytes_read]).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Couldn't parse request: {e}"),
            )
        })
    }
}
