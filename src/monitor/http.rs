//! HTTP status endpoint for runtime monitoring.

use super::stats::SharedStats;
use axum::{Json, Router, routing::get};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

/// Serve the status HTTP endpoint.
pub async fn serve(stats: Arc<SharedStats>, addr: SocketAddr) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/status", get({
            let stats = stats.clone();
            move || {
                let stats = stats.clone();
                async move { Json(stats.snapshot()) }
            }
        }))
        .route("/health", get(|| async { Json(serde_json::json!({"ok": true})) }));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "status endpoint listening");
    axum::serve(listener, app).await?;
    Ok(())
}
