use tokio::{io, net::TcpListener};
use tracing::{debug, info};

pub struct Server {
    address: String,
}

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
            });
        }
    }
}
