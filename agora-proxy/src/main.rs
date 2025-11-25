use agora_proxy::server::{ProxyEntry, Server, ServerConfig};
use regex::Regex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = "0.0.0.0:8080";
    let mut config = ServerConfig::default();
    config.reverse_proxy_mapping.push((
        Regex::new(".*").unwrap(),
        ProxyEntry {
            addr: "127.0.0.1:3000".to_string(),
            strip_prefix: false,
        },
    ));
    // TODO: figure out how to allow user to set up proxy

    let server = Server::new(config);
    server.listen(addr).await?;

    Ok(())
}
