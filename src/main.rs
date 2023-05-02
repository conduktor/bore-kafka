use anyhow::Result;
use clap::{Parser, Subcommand};
use conduktor_kafka_proxy::kafka::KafkaProxy;
use conduktor_kafka_proxy::server::Server;
use conduktor_kafka_proxy::CONDUKTOR_BORE_SERVER;
use tracing::info;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Starts a local LocalProxy to the remote server.
    Start {
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

#[tokio::main]
async fn run(command: Command) -> Result<()> {
    match command {
        Command::Start {
            bootstrap_server,
            secret,
        } => {
            let remote = KafkaProxy::new(CONDUKTOR_BORE_SERVER, secret.as_deref())
                .start(&bootstrap_server)
                .await?;
            info!("Started proxy on {}", remote);
            futures::pending!();
        }
        Command::Server { min_port, secret } => {
            Server::new(min_port, secret.as_deref()).listen().await?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    run(Args::parse().command)
}
