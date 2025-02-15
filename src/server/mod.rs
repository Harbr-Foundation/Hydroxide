#![allow(unused_imports)]

use axum::body::Body;
use axum::http::header;
use axum::routing::IntoMakeService;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, Method, Request, StatusCode, Uri},
    response::IntoResponse,
    routing::{any, get, post},
    Router,
};
#[cfg(feature = "https")]
use axum_server::tls_rustls::RustlsConfig;
use rcgen::generate_simple_self_signed;
use std::future::IntoFuture;
use std::net::SocketAddr;
use dav_server::DavHandler;
use dav_server::fakels::FakeLs;
use dav_server::localfs::LocalFs;
use dav_server::memls::MemLs;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{debug, error, info, trace};
#[derive(Clone)]
struct GitServer {
    instance_url: String,
    addr: SocketAddr,
}

/// Main entry point: chooses between TLS mode and plain HTTP.
#[allow(unused_variables)]
// allow unused variables because we currently only need the config if the https feature
// is enabled
pub async fn launch(config: crate::config::Config) {
    // For TLS mode, config.port is the HTTPS port (e.g. 443)
    let https_addr = SocketAddr::from(([0, 0, 0, 0], 443));
    // Use port 80 for HTTP redirection.
    let http_addr = SocketAddr::from(([0, 0, 0, 0], 80));

    // Build the main application router.
    let main_router = build_main_router();

    if cfg!(feature = "https") {
        // When TLS is enabled, run both the HTTPS server and an HTTP server that redirects to HTTPS.
        #[cfg(feature = "https")]
        {
            let state = GitServer {
                instance_url: format!("https://{}", https_addr),
                addr: SocketAddr::from(([0, 0, 0, 0], 443)),
            };

            let addr = state.addr.clone();

            let https_router = main_router.clone().with_state(state.clone());
            let redirect_state = state.clone();
            let redirect_router = Router::new().fallback(move |req: Request<Body>| {
                let state = redirect_state.clone();
                async move { redirect_fallback_inner(req, state).await }
            });
            let redirect_router = redirect_router.into_make_service();

            let https_server = run_https(addr, https_router.into_make_service());
            match config.http_redirect {
                true => {
                    let http_redirect_server = run_http_redirect(http_addr, redirect_router);
                    tokio::join!(https_server, http_redirect_server);
                }
                false => tokio::task::block_in_place(move || https_server).await,
            }
        }
    } else {
        // Run only a plain HTTP server.
        let state = GitServer {
            instance_url: format!("http://{}", http_addr),
            addr: SocketAddr::from(([0, 0, 0, 0], 80)),
        };

        let addr = state.addr.clone();

        let router = main_router.with_state(state).into_make_service();
        run_http(addr, router).await;
    }
}

/// Build the main application router.
fn build_main_router() -> Router<GitServer> {
    Router::new()
        .route("/init/{user}/{repo_name}", post(init))
        .route("/u/{user}/{repo_name}/{*path}", get(handle_repo))
        .route("/u/{user}/{repo_name}/{*path}", post(handle_repo))
        .route("/u/{user}/{repo_name}/", any(handle_propfind))
        .fallback(fallback)
}

/// Run the HTTPS server using TLS configuration.
#[cfg(feature = "https")]
async fn run_https(addr: SocketAddr, router: IntoMakeService<Router>) {
    let tls_config = make_tls_config().await;
    info!("Starting HTTPS server on {}", addr);
    axum_server::bind_rustls(addr, tls_config)
        .serve(router)
        .await
        .unwrap();
}
/// Create a simple self-signed TLS configuration.
#[cfg(feature = "https")]
async fn make_tls_config() -> RustlsConfig {
    let subject_alt_names = vec!["localhost".to_string()];
    let cert =
        generate_simple_self_signed(subject_alt_names).expect("Failed to generate certificate");

    let cert_der = cert.cert.der();
    let key_der = cert.key_pair.serialized_der().to_vec();

    RustlsConfig::from_der(vec![cert_der.to_vec()], key_der)
        .await
        .expect("Failed to create TLS config")
}

/// Run the HTTP redirect server.
#[cfg(feature = "https")]
async fn run_http_redirect(addr: SocketAddr, router: IntoMakeService<Router>) {
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind HTTP redirect listener");

    info!("Starting HTTP redirect server on {}", addr);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .into_future()
        .await
        .expect("HTTP redirect server failed");
}

/// Run a plain HTTP server.
async fn run_http(addr: SocketAddr, router: IntoMakeService<Router>) {
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind HTTP listener");
    info!("Starting plain HTTP server on {}", addr);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Failed to start server");
}

/// Signal handler for graceful shutdown.
async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Shutting down server...");
}

// REMOVE THIS PLEASE
// A minimal WebDAV response for PROPFIND requests.
async fn handle_propfind() -> impl IntoResponse {
    info!("PROPFIND request");
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<multistatus xmlns="DAV:">
</multistatus>"#;
    let mut headers = HeaderMap::new();
    let _ = headers.insert("Content-Type", "application/xml".parse().unwrap());
    (StatusCode::MULTI_STATUS, headers, body)
}

/// Initialize a bare Git repository.
async fn init(Path((user, repo_name)): Path<(String, String)>) -> impl IntoResponse {
    let repo_path = format!("repos/{}/{}", user, repo_name);
    let repo = git2::Repository::init_bare(&repo_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    match repo {
        Ok(repo) => {
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
    Path((user, repo_name, path)): Path<(String, String, String)>,
    uri: Uri,
) -> impl IntoResponse {
    let repo_path = format!("/repos/{}/{}", user, repo_name);

    let relative_uri = {
        if let Some(query) = uri.query() {
            format!("/{}?{}", path, query)
        } else {
            format!("/{}", path)
        }
    };
    debug!("Repository path: {}, relative URI: {}", repo_path.escape_debug(), relative_uri.escape_debug());

    let dav_server = DavHandler::builder()
        .filesystem(LocalFs::new(repo_path.clone(), true, false, false))
        .locksystem(MemLs::new())
        .build_handler();

    let req = Request::builder()
        .uri(relative_uri)
        .body(Body::empty())
        .unwrap();
    debug!("DAV Request: {:?}", req);
    dav_server.handle(req).await
}


/// Fallback handler for unmatched routes on the main router.
async fn fallback(uri: Uri, State(_state): State<GitServer>, method: Method) -> impl IntoResponse {
    let msg = format!("404 - Not Found: {} {}", method, uri);
    error!("{}", msg);
    (StatusCode::NOT_FOUND, msg)
}

/// Internal function for redirect fallback.
/// This function builds the HTTPS URI and returns a redirect response.
#[allow(dead_code)]
async fn redirect_fallback_inner(req: Request<Body>, state: GitServer) -> impl IntoResponse {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let uri = req.uri();
    let https_uri = {
        let mut parts = uri.clone().into_parts();
        parts.scheme = Some("https".parse().unwrap());
        if host.is_empty() {
            // Fall back to the instance URL from state.
            let instance_host = state
                .instance_url
                .strip_prefix("https://")
                .unwrap_or(&state.instance_url);
            parts.authority = Some(instance_host.parse().unwrap());
        } else {
            parts.authority = Some(host.parse().unwrap());
        }
        Uri::from_parts(parts).unwrap()
    };
    (
        StatusCode::TEMPORARY_REDIRECT,
        [(header::LOCATION, https_uri.to_string())],
    )
}
