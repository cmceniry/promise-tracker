use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::storage::Storage;
use crate::validation::validate_contract;

/// Application state containing the storage
#[derive(Clone)]
pub struct AppState {
    storage: Arc<tokio::sync::RwLock<Storage>>,
}

impl AppState {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage: Arc::new(tokio::sync::RwLock::new(storage)),
        }
    }
}

/// Create the axum router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/contracts", get(list_contracts))
        .route("/contracts/*contract_id", get(get_contract).put(put_contract))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// GET /contracts - List contents of root directory (contracts and subdirectories)
async fn list_contracts(State(state): State<AppState>) -> impl IntoResponse {
    let storage = state.storage.read().await;
    
    match storage.list_directory(None) {
        Ok(entries) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&entries).unwrap()))
                .unwrap()
        }
        Err(e) => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from(format!("Failed to list directory: {}", e)))
                .unwrap()
        }
    }
}

/// GET /contracts/{contract_id} - Get a specific contract or list directory
async fn get_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> impl IntoResponse {
    // URL decode the contract_id (axum Path doesn't decode automatically)
    let contract_id = urlencoding::decode(&contract_id)
        .map(|decoded| decoded.to_string())
        .unwrap_or(contract_id);

    let storage = state.storage.read().await;
    
    // First, check if it's a directory
    if let Ok(entries) = storage.list_directory(Some(&contract_id)) {
        // It's a directory, return the listing
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(&entries).unwrap()))
            .unwrap();
    }
    
    // Otherwise, try to load it as a contract file
    match storage.load_contract(&contract_id) {
        Ok(content) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/x-yaml")
                .body(Body::from(content))
                .unwrap()
        }
        Err(e) => {
            let error_msg = format!("Contract not found: {}", e);
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from(error_msg))
                .unwrap()
        }
    }
}

/// PUT /contracts/{contract_id} - Create or update a contract
async fn put_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // URL decode the contract_id (axum Path doesn't decode automatically)
    let contract_id = urlencoding::decode(&contract_id)
        .map(|decoded| decoded.to_string())
        .unwrap_or(contract_id);

    // Parse the request body as string
    let content = match String::from_utf8(body.to_vec()) {
        Ok(content) => content,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from(format!("Invalid UTF-8 in request body: {}", e)))
                .unwrap();
        }
    };

    // Validate the contract
    match validate_contract(&content) {
        Ok(_) => {
            // Save the contract
            let mut storage = state.storage.write().await;
            match storage.save_contract(&contract_id, &content) {
                Ok(_) => {
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/plain")
                        .body(Body::from("Contract saved successfully"))
                        .unwrap()
                }
                Err(e) => {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header(header::CONTENT_TYPE, "text/plain")
                        .body(Body::from(format!("Failed to save contract: {}", e)))
                        .unwrap()
                }
            }
        }
        Err(e) => {
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from(format!("Validation error: {}", e)))
                .unwrap()
        }
    }
}
