use std::sync::Arc;

use anyhow::Result;
use bore_cli::connection_pool::add_connection;
use bore_cli::{
    connection_pool::{ProxyState, Url},
    server::Server,
};
use clap::{Parser, Subcommand};
use std::sync::RwLock;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Starts a local LocalProxy to the remote server.
    KafkaProxy {
        /// The local host to expose.
        #[clap(
            short,
            long,
            value_name = "BOOTSTRAP_SERVER",
            default_value = "localhost:9092"
        )]
        bootstrap_server: String,

        /// Optional secret for authentication.
        #[clap(short, long, env = "BORE_SECRET", hide_env_values = true)]
        secret: Option<String>,
    },

    /// Runs the remote proxy server.
    Server {
        /// Minimum TCP port number to accept.
        #[clap(long, default_value_t = 1024)]
        min_port: u16,

        /// Optional secret for authentication.
        #[clap(short, long, env = "BORE_SECRET", hide_env_values = true)]
        secret: Option<String>,
    },
}

fn parse_bootstrap_server(bootstrap_server: String) -> Url {
    let mut split = bootstrap_server.split(":");
    let host = split.next().unwrap();
    let port = split.next().unwrap().parse().unwrap();
    Url::new(host.to_string(), port)
}

#[tokio::main]
async fn run(command: Command) -> Result<()> {
    match command {
        Command::KafkaProxy {
            bootstrap_server,
            secret,
        } => {
            let proxy_state = Arc::new(RwLock::new(ProxyState::new(secret)));
            add_connection(&proxy_state, parse_bootstrap_server(bootstrap_server)).await;
            return_infinite_future().await;
        }
        Command::Server { min_port, secret } => {
            Server::new(min_port, secret.as_deref()).listen().await?;
        }
    }

    Ok(())
}

async fn return_infinite_future() {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    run(Args::parse().command)
}
