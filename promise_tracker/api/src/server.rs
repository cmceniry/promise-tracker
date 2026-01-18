use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::static_files::serve_static_or_proxy;
use crate::storage::{DirectoryEntry, EntryType, Storage};
use crate::validation::validate_contract;

/// Application state containing the storage
#[derive(Clone)]
pub struct AppState {
    storage: Arc<tokio::sync::RwLock<Storage>>,
    dev_mode: bool,
    dev_server_url: Arc<String>,
}

impl AppState {
    pub fn new(storage: Storage, dev_mode: bool, dev_server_url: String) -> Self {
        Self {
            storage: Arc::new(tokio::sync::RwLock::new(storage)),
            dev_mode,
            dev_server_url: Arc::new(dev_server_url),
        }
    }
}

/// Check if the request prefers HTML based on Accept header
fn prefers_html(headers: &HeaderMap) -> bool {
    if let Some(accept) = headers.get(header::ACCEPT) {
        if let Ok(accept_str) = accept.to_str() {
            // Check if text/html is preferred over application/json or application/x-yaml
            let accept_lower = accept_str.to_lowercase();
            // If text/html appears before application/json or application/x-yaml, prefer HTML
            // Also check for wildcard text/* or */*
            if accept_lower.contains("text/html") {
                // Check if HTML has higher quality than JSON/YAML
                // Simple heuristic: if text/html appears, prefer it unless explicitly weighted lower
                return true;
            }
        }
    }
    false
}

/// Render directory listing as HTML
fn render_directory_html(entries: &[DirectoryEntry], current_path: Option<&str>) -> String {
    let path_display = current_path.unwrap_or("/contracts");
    let is_root = current_path.is_none();

    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<title>Contracts - ");
    html.push_str(&html_escape(path_display));
    html.push_str("</title>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: system-ui, -apple-system, sans-serif; margin: 40px; background: #f5f5f5; }\n");
    html.push_str(".container { max-width: 800px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }\n");
    html.push_str("h1 { margin-top: 0; color: #333; }\n");
    html.push_str(".path { color: #666; margin-bottom: 20px; font-size: 14px; }\n");
    html.push_str("ul { list-style: none; padding: 0; }\n");
    html.push_str("li { padding: 8px 0; border-bottom: 1px solid #eee; }\n");
    html.push_str("li:last-child { border-bottom: none; }\n");
    html.push_str("a { text-decoration: none; color: #0066cc; }\n");
    html.push_str("a:hover { text-decoration: underline; }\n");
    html.push_str(".dir::before { content: '📁 '; }\n");
    html.push_str(".file::before { content: '📄 '; }\n");
    html.push_str(".parent::before { content: '⬆️ '; font-weight: bold; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n<div class=\"container\">\n");
    html.push_str("<h1>Contracts</h1>\n");
    html.push_str("<div class=\"path\">");
    html.push_str(&html_escape(path_display));
    html.push_str("</div>\n");
    html.push_str("<ul>\n");

    // Add ".." entry if not at root
    if !is_root {
        let parent_path = if let Some(path) = current_path {
            // Calculate parent path
            if let Some(last_slash) = path.rfind('/') {
                if last_slash == 0 {
                    "/contracts".to_string()
                } else {
                    let parent = &path[..last_slash];
                    // Encode the parent path segments
                    let encoded_parent: String = parent
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(|s| urlencoding::encode(s))
                        .collect::<Vec<_>>()
                        .join("/");
                    format!("/contracts/{}", encoded_parent)
                }
            } else {
                // No slash found, parent is root
                "/contracts".to_string()
            }
        } else {
            "/contracts".to_string()
        };
        html.push_str("<li class=\"parent\"><a href=\"");
        html.push_str(&parent_path);
        html.push_str("\">..</a></li>\n");
    }

    // Add directory entries
    for entry in entries {
        let entry_name = &entry.name;
        let entry_url = if let Some(path) = current_path {
            // Encode both the current path and the entry name
            let encoded_path: String = path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| urlencoding::encode(s))
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "/contracts/{}/{}",
                encoded_path,
                urlencoding::encode(entry_name)
            )
        } else {
            format!("/contracts/{}", urlencoding::encode(entry_name))
        };

        let class = match entry.entry_type {
            EntryType::Directory => "dir",
            EntryType::Contract => "file",
        };

        html.push_str("<li class=\"");
        html.push_str(class);
        html.push_str("\"><a href=\"");
        html.push_str(&entry_url);
        html.push_str("\">");
        html.push_str(&html_escape(entry_name));
        html.push_str("</a></li>\n");
    }

    html.push_str("</ul>\n</div>\n</body>\n</html>");
    html
}

/// Render contract content as HTML
fn render_contract_html(contract_id: &str, content: &str) -> String {
    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<title>Contract - ");
    html.push_str(&html_escape(contract_id));
    html.push_str("</title>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: system-ui, -apple-system, sans-serif; margin: 40px; background: #f5f5f5; }\n");
    html.push_str(".container { max-width: 1000px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }\n");
    html.push_str("h1 { margin-top: 0; color: #333; }\n");
    html.push_str(".path { color: #666; margin-bottom: 20px; font-size: 14px; }\n");
    html.push_str("pre { background: #f8f8f8; padding: 20px; border-radius: 4px; overflow-x: auto; border: 1px solid #ddd; }\n");
    html.push_str("code { font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace; font-size: 13px; line-height: 1.5; }\n");
    html.push_str("a { color: #0066cc; text-decoration: none; }\n");
    html.push_str("a:hover { text-decoration: underline; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n<div class=\"container\">\n");
    html.push_str("<h1>Contract</h1>\n");
    html.push_str("<div class=\"path\">");

    // Add breadcrumb navigation
    if let Some(last_slash) = contract_id.rfind('/') {
        let parent_path = &contract_id[..last_slash];
        html.push_str("<a href=\"/contracts\">/contracts</a>");
        let mut segments: Vec<&str> = Vec::new();
        for segment in parent_path.split('/') {
            if !segment.is_empty() {
                segments.push(segment);
                let encoded_path: String = segments
                    .iter()
                    .map(|s| urlencoding::encode(s))
                    .collect::<Vec<_>>()
                    .join("/");
                html.push_str(" / <a href=\"/contracts/");
                html.push_str(&encoded_path);
                html.push_str("\">");
                html.push_str(&html_escape(segment));
                html.push_str("</a>");
            }
        }
        html.push_str(" / ");
        html.push_str(&html_escape(&contract_id[last_slash + 1..]));
    } else {
        html.push_str("<a href=\"/contracts\">/contracts</a> / ");
        html.push_str(&html_escape(contract_id));
    }

    html.push_str("</div>\n");
    html.push_str("<pre><code>");
    html.push_str(&html_escape(content));
    html.push_str("</code></pre>\n");
    html.push_str("</div>\n</body>\n</html>");
    html
}

/// Render error as HTML
fn render_error_html(status: StatusCode, message: &str) -> String {
    let is_success = status.is_success();
    let title = if is_success { "Success" } else { "Error" };
    let title_color = if is_success { "#2e7d32" } else { "#d32f2f" };

    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<title>");
    html.push_str(title);
    html.push_str("</title>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: system-ui, -apple-system, sans-serif; margin: 40px; background: #f5f5f5; }\n");
    html.push_str(".container { max-width: 600px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }\n");
    html.push_str("h1 { margin-top: 0; color: ");
    html.push_str(title_color);
    html.push_str("; }\n");
    html.push_str(".message { color: #666; margin: 20px 0; }\n");
    html.push_str("a { color: #0066cc; text-decoration: none; }\n");
    html.push_str("a:hover { text-decoration: underline; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n<div class=\"container\">\n");
    html.push_str("<h1>");
    html.push_str(title);
    if !is_success {
        html.push_str(" ");
        html.push_str(&status.as_str());
    }
    html.push_str("</h1>\n");
    html.push_str("<div class=\"message\">");
    html.push_str(&html_escape(message));
    html.push_str("</div>\n");
    html.push_str("<p><a href=\"/contracts\">← Back to contracts</a></p>\n");
    html.push_str("</div>\n</body>\n</html>");
    html
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '&' => "&amp;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            _ => c.to_string(),
        })
        .collect::<String>()
}

/// Create the axum router with all routes
pub fn create_router(state: AppState) -> Router {
    // Conditional CORS (only needed in dev mode)
    let cors_layer = if state.dev_mode {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
    };

    Router::new()
        // API routes - checked first
        .route("/contracts", get(list_contracts))
        .route(
            "/contracts/*contract_id",
            get(get_contract).put(put_contract),
        )
        .layer(cors_layer)
        // Fallback to static files for non-API routes
        .fallback(static_file_handler)
        .with_state(state)
}

/// Handler for static files
async fn static_file_handler(State(state): State<AppState>, uri: Uri) -> Response {
    serve_static_or_proxy(uri, state.dev_mode, &state.dev_server_url).await
}

/// GET /contracts - List contents of root directory (contracts and subdirectories)
async fn list_contracts(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let storage = state.storage.read().await;
    let wants_html = prefers_html(&headers);

    match storage.list_directory(None) {
        Ok(entries) => {
            if wants_html {
                let html = render_directory_html(&entries, None);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&entries).unwrap()))
                    .unwrap()
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to list directory: {}", e);
            if wants_html {
                let html = render_error_html(StatusCode::INTERNAL_SERVER_ERROR, &error_msg);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from(error_msg))
                    .unwrap()
            }
        }
    }
}

/// GET /contracts/{contract_id} - Get a specific contract or list directory
async fn get_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // URL decode the contract_id (axum Path doesn't decode automatically)
    let contract_id = urlencoding::decode(&contract_id)
        .map(|decoded| decoded.to_string())
        .unwrap_or(contract_id);

    let storage = state.storage.read().await;
    let wants_html = prefers_html(&headers);

    // First, check if it's a directory
    if let Ok(entries) = storage.list_directory(Some(&contract_id)) {
        // It's a directory, return the listing
        if wants_html {
            let html = render_directory_html(&entries, Some(&contract_id));
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(html))
                .unwrap();
        } else {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&entries).unwrap()))
                .unwrap();
        }
    }

    // Otherwise, try to load it as a contract file
    match storage.load_contract(&contract_id) {
        Ok(content) => {
            if wants_html {
                let html = render_contract_html(&contract_id, &content);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/x-yaml")
                    .body(Body::from(content))
                    .unwrap()
            }
        }
        Err(e) => {
            let error_msg = format!("Contract not found: {}", e);
            if wants_html {
                let html = render_error_html(StatusCode::NOT_FOUND, &error_msg);
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from(error_msg))
                    .unwrap()
            }
        }
    }
}

/// PUT /contracts/{contract_id} - Create or update a contract
async fn put_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // URL decode the contract_id (axum Path doesn't decode automatically)
    let contract_id = urlencoding::decode(&contract_id)
        .map(|decoded| decoded.to_string())
        .unwrap_or(contract_id);

    let wants_html = prefers_html(&headers);

    // Parse the request body as string
    let content = match String::from_utf8(body.to_vec()) {
        Ok(content) => content,
        Err(e) => {
            let error_msg = format!("Invalid UTF-8 in request body: {}", e);
            if wants_html {
                let html = render_error_html(StatusCode::BAD_REQUEST, &error_msg);
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap();
            } else {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from(error_msg))
                    .unwrap();
            }
        }
    };

    // Validate the contract
    match validate_contract(&content) {
        Ok(_) => {
            // Save the contract
            let mut storage = state.storage.write().await;
            match storage.save_contract(&contract_id, &content) {
                Ok(_) => {
                    if wants_html {
                        let html = render_error_html(StatusCode::OK, "Contract saved successfully");
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                            .body(Body::from(html))
                            .unwrap()
                    } else {
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "text/plain")
                            .body(Body::from("Contract saved successfully"))
                            .unwrap()
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to save contract: {}", e);
                    if wants_html {
                        let html = render_error_html(StatusCode::INTERNAL_SERVER_ERROR, &error_msg);
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                            .body(Body::from(html))
                            .unwrap()
                    } else {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .header(header::CONTENT_TYPE, "text/plain")
                            .body(Body::from(error_msg))
                            .unwrap()
                    }
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Validation error: {}", e);
            if wants_html {
                let html = render_error_html(StatusCode::BAD_REQUEST, &error_msg);
                Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(html))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from(error_msg))
                    .unwrap()
            }
        }
    }
}
