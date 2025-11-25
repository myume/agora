use agora_proxy::server::{ProxyEntry, Server, ServerConfig};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the server
    Start {
        #[arg(short, long, default_value_t = 8080)]
        /// The port the server should listen on
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.command {
        Commands::Start { port } => run(port).await,
    }
}

async fn run(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = format!("0.0.0.0:{}", port);
    let mut config = ServerConfig::default();
    config.reverse_proxy_mapping.push((
        String::from("/"),
        ProxyEntry {
            addr: "127.0.0.1:3000".to_string(),
            strip_prefix: false,
        },
    ));
    // TODO: figure out how to allow user to set up proxy

    let server = Server::new(config);
    server.listen(&addr).await?;

    Ok(())
}
