use std::io;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, Layer, Registry};

pub mod cli;
pub mod server;

pub mod config;
#[tokio::main]
async fn main() {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "app.log");

    let stdout_layer = fmt::layer()
        .with_writer(io::stdout)
        .with_ansi(true)
        .pretty()
        .without_time()
        .with_filter(tracing_subscriber::filter::LevelFilter::TRACE);

    let file_layer = fmt::layer().with_writer(file_appender).with_ansi(false);

    let subscriber = Registry::default().with(stdout_layer).with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");
    cli::run().await;
}
