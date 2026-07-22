use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::json;

use crate::infrastructure::app_state::AppState;

/// Liveness: the process is up and serving. Never touches dependencies, so an
/// orchestrator will not restart the pod when Postgres has a hiccup.
pub async fn liveness() -> &'static str {
    "🚀 EngOS Server is running!"
}

/// Readiness: the process can actually serve traffic.
///
/// The original `/` handler returned a static string, so a container with a
/// dead database still reported healthy.
pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.health_check().await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "status": "ok", "postgres": "up", "redis": "up" })),
        ),
        Err(error) => {
            tracing::error!(%error, kind = error.kind(), "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "degraded", "reason": error.kind() })),
            )
        }
    }
}
