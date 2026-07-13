use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use crate::infrastructure::app_state::AppState;
use crate::infrastructure::http::line_webhook::handle_webhook;

pub async fn start_server(state: AppState, host: &str, port: u16) -> Result<(), String> {
    let app = Router::new()
        .route("/webhook", post(handle_webhook))
        .route("/", get(|| async { "🚀 EngOS Server is running!" }))
        .with_state(state);

    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| format!("Invalid host or port address ({}): {}", addr_str, e))?;

    println!("🌟 EngOS Server is listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind TCP listener to {}: {}", addr, e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}