use crate::server::Server;

mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = String::from("127.0.0.1:8080");
    let server = Server::new(addr);

    server.listen().await?;

    Ok(())
}
