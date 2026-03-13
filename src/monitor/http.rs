//! HTTP status endpoint for runtime monitoring and path management.

use super::stats::{SharedStats, StatsSnapshot};
use axum::{Json, Router, extract::State, routing::{delete, get, post}};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

/// Command to add or remove a path at runtime.
#[derive(Debug, Clone)]
pub enum PathCommand {
    Add {
        bind_addr: SocketAddr,
        peer_addr: Option<SocketAddr>,
    },
    Remove {
        path_id: u32,
    },
}

/// Response for path management endpoints.
#[derive(Serialize)]
struct PathResponse {
    ok: bool,
    message: String,
}

/// Request body for adding a path.
#[derive(Deserialize)]
struct AddPathRequest {
    bind_addr: String,
    peer_addr: Option<String>,
}

#[derive(Clone)]
struct AppState {
    stats: Arc<SharedStats>,
    path_cmd_tx: mpsc::Sender<PathCommand>,
}

/// Serve the status HTTP endpoint with path management API.
pub async fn serve(
    stats: Arc<SharedStats>,
    addr: SocketAddr,
    path_cmd_tx: mpsc::Sender<PathCommand>,
) -> anyhow::Result<()> {
    let state = AppState { stats, path_cmd_tx };

    let app = Router::new()
        .route("/status", get(handle_status))
        .route("/health", get(handle_health))
        .route("/paths", post(handle_add_path))
        .route("/paths/{id}", delete(handle_remove_path))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "status endpoint listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_status(State(state): State<AppState>) -> Json<StatsSnapshot> {
    Json(state.stats.snapshot())
}

async fn handle_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true}))
}

async fn handle_add_path(
    State(state): State<AppState>,
    Json(req): Json<AddPathRequest>,
) -> Json<PathResponse> {
    let bind_addr: SocketAddr = match req.bind_addr.parse() {
        Ok(a) => a,
        Err(e) => return Json(PathResponse {
            ok: false,
            message: format!("invalid bind_addr: {e}"),
        }),
    };
    let peer_addr: Option<SocketAddr> = match req.peer_addr {
        Some(ref s) => match s.parse() {
            Ok(a) => Some(a),
            Err(e) => return Json(PathResponse {
                ok: false,
                message: format!("invalid peer_addr: {e}"),
            }),
        },
        None => None,
    };

    match state.path_cmd_tx.send(PathCommand::Add { bind_addr, peer_addr }).await {
        Ok(()) => Json(PathResponse {
            ok: true,
            message: "path add queued".into(),
        }),
        Err(_) => Json(PathResponse {
            ok: false,
            message: "runtime channel closed".into(),
        }),
    }
}

async fn handle_remove_path(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> Json<PathResponse> {
    match state.path_cmd_tx.send(PathCommand::Remove { path_id: id }).await {
        Ok(()) => Json(PathResponse {
            ok: true,
            message: format!("path {id} remove queued"),
        }),
        Err(_) => Json(PathResponse {
            ok: false,
            message: "runtime channel closed".into(),
        }),
    }
}
