use agora_proxy::server::{Server, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = "127.0.0.1:8080";
    let config = ServerConfig::default();
    // TODO: figure out how to allow user to set up proxy

    let server = Server::new(config);

    server.listen(addr).await?;

    Ok(())
}
