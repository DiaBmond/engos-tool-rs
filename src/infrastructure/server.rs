use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
use tokio::signal;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::domain::error::{AppError, AppResult};
use crate::infrastructure::app_state::AppState;
use crate::infrastructure::http::health::{liveness, readiness};
use crate::infrastructure::http::line_webhook::handle_webhook;

/// LINE webhook bodies are small; anything larger is not a legitimate request.
const MAX_BODY_BYTES: usize = 256 * 1024;

/// The handler acknowledges within milliseconds because the real work is
/// spawned, so this only needs to bound pathological cases.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/webhook", post(handle_webhook::<AppState>))
        .route("/", get(liveness))
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .with_state(state)
        // A panic inside a handler would otherwise kill the connection without
        // a response, which LINE reads as a failure and retries.
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
}

fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    use axum::response::IntoResponse;

    let detail = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");

    tracing::error!(panic = detail, "handler panicked");

    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
}

pub async fn start_server(state: AppState, host: &str, port: u16) -> AppResult<()> {
    let addr_str = format!("{host}:{port}");
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| AppError::Config(format!("invalid bind address {addr_str}: {e}")))?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Config(format!("failed to bind {addr}: {e}")))?;

    tracing::info!(%addr, "EngOS server listening");

    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| AppError::Config(format!("server error: {e}")))?;

    tracing::info!("server shut down cleanly");
    Ok(())
}

/// Resolves on SIGINT or SIGTERM so in-flight requests can finish instead of
/// being cut off mid-turn during a deploy.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
