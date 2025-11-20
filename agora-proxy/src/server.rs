use agora_http_parser::Request;
use tokio::{
    io::{self},
    net::TcpListener,
};
use tracing::{debug, info};

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
                debug!("Connection Accepted: {addr}");

                stream.readable().await.unwrap();

                let mut buf = [0; MAX_BUF_SIZE];
                let mut buf_size = 0;
                let request = loop {
                    match stream.try_read(&mut buf) {
                        Ok(n) => {
                            buf_size += n;
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        Err(e) => {
                            eprintln!("{e}");
                        }
                    }

                    match Request::parse(&buf[..buf_size]) {
                        Ok(request) => break request,
                        Err(ref e) => eprintln!("{:?}", e),
                    }
                };

                debug!("{request}");
            });
        }
    }
}
