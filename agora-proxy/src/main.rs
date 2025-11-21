use crate::server::{Server, ServerConfig};

mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = "127.0.0.1:8080";
    let config = ServerConfig::default();
    let server = Server::new(config);

    server.listen(addr).await?;

    Ok(())
}
