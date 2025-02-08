use crate::config::Port;
use clap::{Parser, Subcommand};
use tracing::trace;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
    #[command(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}
#[derive(Debug, Clone, Subcommand)]
enum Commands {
    #[command(alias = "up")]
    #[command(alias = "start")]
    Run { port: Option<u16> },
}

pub async fn run() {
    let args = Cli::try_parse();

    match args {
        Ok(cli) => match cli.command {
            Commands::Run { port, .. } => {
                let config = crate::config::Builder::new();
                let config = if let Some(port) = port {
                    config.with_port(Port(port))
                } else {
                    trace!("No custom port was specified, defaulting to 80");
                    config
                };
                let server_config = config.clone().build();
                crate::server::launch(server_config).await;
            }
        },
        Err(e) => {
            e.exit();
        }
    }
}
