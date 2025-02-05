#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use axum::{http::StatusCode, response::IntoResponse, Router};
use std::io;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_appender::rolling::{daily, RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, Registry};

#[tokio::main]
async fn main() {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "app.log");

    let stdout_layer = fmt::layer().with_writer(io::stdout).with_ansi(true);

    let file_layer = fmt::layer().with_writer(file_appender).with_ansi(false);

    let subscriber = Registry::default().with(stdout_layer).with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");

    let app = Router::new().fallback(fallback);

    let listener = TcpListener::bind("0.0.0.0:80").await.unwrap();
    let addr = listener.local_addr().unwrap();

    info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

// Graceful shutdown handler.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Shutting down server...");
}

async fn fallback(uri: axum::http::Uri) -> impl IntoResponse {
    error!("404 - Not Found: {}", uri);
    (StatusCode::NOT_FOUND, "404 - Not Found")
}
