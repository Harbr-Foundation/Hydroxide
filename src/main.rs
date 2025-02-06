#![allow(unused_imports)]
use axum::extract::State;
use axum::http::{Method, Uri};
use axum::routing::{get, post};
use axum::{http::StatusCode, response::IntoResponse, Router, ServiceExt};
#[cfg(feature = "https")]
use axum_server::tls_rustls::RustlsConfig;
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format;
use tracing_subscriber::layer::Filter;
use tracing_subscriber::{fmt, prelude::*, Registry};

#[derive(Clone)]
#[allow(dead_code)]
struct GitServer {
    instance_url: String,
    router: Router<GitServer>,
    addr: SocketAddr,
}
#[tokio::main]
async fn main() {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "app.log");

    let stdout_layer = fmt::layer()
        .with_writer(io::stdout)
        .with_ansi(true)
        .pretty()
        .without_time()
        .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);

    let file_layer = fmt::layer().with_writer(file_appender).with_ansi(false);

    let subscriber = Registry::default().with(stdout_layer).with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");

    let listener = TcpListener::bind("0.0.0.0:80").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let router = Router::new()
        .route("/init/{user}/{repo_name}", post(init))
        .route("/u/{user}/{repo_name}/{*path}", get(handle_repo))
        .fallback(fallback);
    let state = GitServer {
        addr,
        instance_url: format!("http://{}", addr),
        router: router.clone(),
    };
    let router = router.with_state(state);

    info!("Server listening on {}", addr);

    if cfg!(feature = "https") {
        #[cfg(feature = "https")]
        {
            serve_tls(addr, router).await;
        }
    } else {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    }
}
#[cfg(feature = "https")]
async fn serve_tls(addr: std::net::SocketAddr, app: Router<GitServer>) {
    let config = RustlsConfig::from_pem_file(
        "examples/self-signed-certs/cert.pem",
        "examples/self-signed-certs/key.pem",
    )
    .await
    .unwrap();
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Shutting down server...");
}

async fn init(
    axum::extract::Path((user, repo_name)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let repo_path = format!("repos/{}/{}", user, repo_name);
    let repo = git2::Repository::init_bare(&repo_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    match repo {
        Ok(repo) => {
            // Create the info/refs file
            let refs_path = format!("{}/info/refs", repo_path);
            if let Err(e) = std::fs::File::create(&refs_path) {
                error!("Failed to create info/refs file: {:#?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to initialize repository".to_string(),
                )
                    .into_response();
            }
            info!("Initialized repository: {}", repo.path().display());
            (
                StatusCode::CREATED,
                format!("Initialized repository: {}", repo.path().display()),
            )
                .into_response()
        }
        Err(e) => {
            error!("Failed to initialize repository: {:#?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to initialize repository".to_string(),
            )
                .into_response()
        }
    }
}
async fn handle_repo(
    axum::extract::Path((user, repo_name, path)): axum::extract::Path<(String, String, String)>,
) -> impl IntoResponse {
    let repo_path = format!("repos/{}/{}", user, repo_name);
    let file_path = format!("{}/{}", repo_path, path);

    match tokio::fs::metadata(&file_path).await {
        Ok(metadata) => {
            if metadata.is_dir() {
                info!("Directory: {}", file_path);
                (StatusCode::OK, format!("Directory: {}", file_path)).into_response()
            } else {
                match tokio::fs::read(&file_path).await {
                    Ok(contents) => (StatusCode::OK, contents).into_response(),
                    Err(_) => (
                        StatusCode::NOT_FOUND,
                        format!("File not found: {}", file_path),
                    )
                        .into_response(),
                }
            }
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("Path not found: {}", file_path),
        )
            .into_response(),
    }
}
async fn fallback(
    uri: axum::http::Uri,
    State(_state): State<GitServer>,
    method: axum::http::Method,
) -> impl IntoResponse {
    let msg = format!("404 - Not Found: {} {}", method, uri);
    error!("{}", msg);
    if let Some(uri) = uri.query() {
        let uri = uri.to_string();
        if uri.contains("service=git") {
            // let instance_url = state.instance_url;
        }
    }
    error!("{}", msg);
    (StatusCode::NOT_FOUND, msg)
}
