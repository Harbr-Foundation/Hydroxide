#![allow(unused_imports)]
use axum::extract::State;
use axum::http::{HeaderMap, Method, Uri};
use axum::routing::{any, get, post};
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

pub async fn launch(config: crate::config::Config) {
    let port = *config.port;
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let router = Router::new()
        .route("/init/{user}/{repo_name}", post(init))
        .route("/u/{user}/{repo_name}/{*path}", get(handle_repo))
        .route("/u/{user}/{repo_name}/{*path}", post(handle_repo))
        .route("/u/{user}/{repo_name}/", any(handle_propfind))
        .route("/u/{user}/{repo_name}/{*path}", any(handle_propfind))
        .fallback(fallback);
    let state = GitServer {
        addr,
        instance_url: format!("http://{}", addr),
        router: router.clone(),
    };
    let router = router.with_state(state);

    info!("Server listening on {}", addr);

    if config.use_https {
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
    // axum_server::bind_rustls(addr, config)
    //     .serve(app.into_make_service())
    //     .with_graceful_shutdown(shutdown_signal())
    //     .await
    //     .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Shutting down server...");
}
async fn handle_propfind() -> impl IntoResponse {
    info!("PROPFIND request");
    // Return a minimal valid WebDAV XML response
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<multistatus xmlns="DAV:">
</multistatus>"#;
    let mut headers = HeaderMap::new();
    let _ = headers.insert("Content-Type", "application/xml".parse().unwrap());
    // A 207 Multi-Status is common for WebDAV responses.
    (StatusCode::MULTI_STATUS, headers, body)
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
    if let Some(uri) = uri.query() {
        let uri = uri.to_string();
        if uri.contains("service=git") {
            // let instance_url = state.instance_url;
        }
    }
    error!("{}", msg);
    (StatusCode::NOT_FOUND, msg)
}
