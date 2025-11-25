use std::path::PathBuf;

use agora_proxy::server::{Server, ServerConfig};
use clap::{Parser, Subcommand};
use tracing::info;

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

        #[arg(short, long)]
        /// Path to server config
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.command {
        Commands::Start { port, config } => run(port, config).await,
    }
}

async fn run(port: u16, config_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = format!("0.0.0.0:{}", port);
    let config = if let Some(config_path) = config_path {
        info!("Loading server config from {}", config_path.display());
        ServerConfig::parse(&config_path)?
    } else {
        info!("No config found: loading default config.");
        ServerConfig::default()
    };

    let server = Server::new(config);
    server.listen(&addr).await?;

    Ok(())
}
