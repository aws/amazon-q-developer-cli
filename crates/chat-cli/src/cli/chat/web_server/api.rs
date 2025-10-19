use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

use super::server::AppState;
use super::events::WorkerLifecycleState;

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// List all workers
pub async fn list_workers(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.session.get_workers();
    
    let worker_infos: Vec<WorkerInfo> = workers
        .iter()
        .map(|w| {
            let lifecycle_state = *w.lifecycle_state.lock().unwrap();
            
            WorkerInfo {
                worker_id: w.id.to_string(),
                name: w.name.clone(),
                lifecycle_state: lifecycle_state.into(),
            }
        })
        .collect();
    
    Json(WorkersResponse {
        workers: worker_infos,
    })
}

/// Get specific worker details
pub async fn get_worker(
    Path(worker_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let worker_id = Uuid::parse_str(&worker_id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid worker ID".to_string(),
            }),
        )
    })?;
    
    let worker = state.session.get_worker(worker_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Worker not found".to_string(),
            }),
        )
    })?;
    
    let lifecycle_state = *worker.lifecycle_state.lock().unwrap();
    
    Ok(Json(WorkerDetailResponse {
        worker_id: worker.id.to_string(),
        name: worker.name.clone(),
        lifecycle_state: lifecycle_state.into(),
    }))
}

#[derive(Debug, Serialize)]
struct WorkersResponse {
    workers: Vec<WorkerInfo>,
}

#[derive(Debug, Serialize)]
struct WorkerInfo {
    worker_id: String,
    name: String,
    lifecycle_state: WorkerLifecycleState,
}

#[derive(Debug, Serialize)]
struct WorkerDetailResponse {
    worker_id: String,
    name: String,
    lifecycle_state: WorkerLifecycleState,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
