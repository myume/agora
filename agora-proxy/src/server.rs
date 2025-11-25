use std::net::SocketAddr;

use agora_http_parser::{HTTPVersion, Request, Response, is_terminated};
use http::StatusCode;
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
    pub strip_prefix: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ServerConfig {
    /// Mapping of Path prefix to proxy entry
    pub reverse_proxy_mapping: Vec<(String, ProxyEntry)>,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn listen(&self, address: &str) -> io::Result<()> {
        let listener = TcpListener::bind(address).await?;
        info!("Listening on {}", address);
        loop {
            let (stream, addr) = listener.accept().await?;

            let config = self.config.clone();
            tokio::spawn(async move {
                Self::process(stream, addr, config).await;
            });
        }
    }

    async fn process(mut client_stream: TcpStream, addr: SocketAddr, config: ServerConfig) {
        debug!("Connection Accepted: {addr}");

        let mut buf = [0; MAX_BUF_SIZE];
        let (mut request, remaining_body) = match read_request(&mut client_stream, &mut buf).await {
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
                close_connection_with_reason(&mut client_stream, reason).await;
                return;
            }
        };

        debug!("{request}");

        if request.version != HTTPVersion::HTTP1_1 {
            close_connection_with_reason(
                &mut client_stream,
                StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            )
            .await;
            return;
        }

        // could be a performance issue iterating through lots of mappings
        let mut proxied_request = false;
        for (prefix, entry) in config.reverse_proxy_mapping {
            if request.path.starts_with(&prefix) {
                debug!("Proxying request to {}", entry.addr);
                proxied_request = true;

                let Ok(mut server_stream) = TcpStream::connect(&entry.addr).await else {
                    error!(
                        "Failed to establish TCP connection with server: {}",
                        entry.addr
                    );
                    close_connection_with_reason(&mut client_stream, StatusCode::BAD_GATEWAY).await;
                    return;
                };

                let mut proxy_conn = ProxyConnection::new(&mut client_stream, &mut server_stream);

                if entry.strip_prefix {
                    request.path = request.path.replace(&prefix, "").to_string();
                    if !request.path.starts_with('/') {
                        request.path.insert(0, '/');
                    }
                }

                if let Err(e) = proxy_conn.proxy_request(request, remaining_body).await {
                    error!("Failed to proxy request to {}: {e}", entry.addr);
                    close_connection_with_reason(&mut client_stream, StatusCode::BAD_GATEWAY).await;
                    return;
                };

                if let Err(e) = proxy_conn.proxy_response(&mut buf).await {
                    error!("Failed to proxy response to {addr}: {e}");
                    close_connection_with_reason(&mut client_stream, StatusCode::BAD_GATEWAY).await;
                    return;
                };

                // Notice that if multiple mappings match the same path,
                // the first one in the array will be chosen.
                break;
            }
        }

        if !proxied_request {
            close_connection_with_reason(&mut client_stream, StatusCode::NOT_FOUND).await;
        }
    }
}

async fn close_connection_with_reason(stream: &mut TcpStream, status_code: StatusCode) {
    let mut response = Response::new(status_code);
    response.header("Connection", "close");
    send_response(stream, response).await;
}

async fn send_response(stream: &mut TcpStream, response: Response) {
    if let Err(e) = stream.write_all(&response.into_bytes()).await {
        error!("Failed to send response: {e}");
    };
}

async fn read_message_into_buffer(
    stream: &mut TcpStream,
    buf: &mut [u8; MAX_BUF_SIZE],
) -> io::Result<usize> {
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
                    "Couldn't parse message",
                ));
            }
            Ok(n) => {
                total_bytes_read += n;
                recent_bytes_read = n;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(total_bytes_read)
}

async fn read_response<'buf>(
    stream: &mut TcpStream,
    buf: &'buf mut [u8; MAX_BUF_SIZE],
) -> io::Result<(Response, &'buf [u8])> {
    let total_bytes_read = read_message_into_buffer(stream, buf).await?;
    Response::parse(&buf[..total_bytes_read]).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Couldn't parse response: {e}"),
        )
    })
}

async fn read_request<'buf>(
    stream: &mut TcpStream,
    buf: &'buf mut [u8; MAX_BUF_SIZE],
) -> io::Result<(Request, &'buf [u8])> {
    let total_bytes_read = read_message_into_buffer(stream, buf).await?;
    Request::parse(&buf[..total_bytes_read]).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Couldn't parse request: {e}"),
        )
    })
}

pub struct ProxyConnection<'conn> {
    client: &'conn mut TcpStream,
    server: &'conn mut TcpStream,
}

impl<'conn> ProxyConnection<'conn> {
    pub fn new(client: &'conn mut TcpStream, server: &'conn mut TcpStream) -> Self {
        Self { client, server }
    }

    pub async fn proxy_request(
        &mut self,
        mut request: Request,
        remaining_bytes: &[u8],
    ) -> io::Result<()> {
        // For now, assume that the full request fits into our buffer.
        // We will need to amend this assumption later, once we get the proxy working.

        if let Ok(client_addr) = self.client.peer_addr() {
            request
                .headers
                .insert("X-Forwarded-For".to_lowercase(), client_addr.to_string());
        }

        let mut request_bytes = request.into_bytes();
        request_bytes.extend(remaining_bytes);
        self.server.write_all(&request_bytes).await
    }

    pub async fn proxy_response(&mut self, buf: &mut [u8; MAX_BUF_SIZE]) -> io::Result<()> {
        let (mut response, remaining) = read_response(self.server, buf).await?;
        debug!("{response}");

        // set connection close for now and just blindly keep reading
        // until the server closes the connection for us.
        // will need to handle content length and transfer encoding chunked in the future.
        response.header("Connection", "close");

        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(remaining);
        self.client.write_all(&bytes).await?;

        while let Ok(bytes_read) = self.server.read(buf).await
            && bytes_read != 0
        {
            self.client.write_all(&buf[..bytes_read]).await?;
        }

        Ok(())
    }
}
