use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::Response,
};
use rust_embed::RustEmbed;
use std::borrow::Cow;

/// Embedded static files from Leptos build
#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
#[include = "*.html"]
#[include = "*.js"]
#[include = "*.css"]
#[include = "*.json"]
#[include = "*.ico"]
#[include = "*.png"]
#[include = "*.svg"]
#[include = "*.txt"]
#[include = "*.wasm"]
#[include = "snippets/**/*"]
pub struct StaticAssets;

impl StaticAssets {
    /// Get a file by path, handling SPA fallback logic
    pub fn get_with_fallback(path: &str) -> Option<(Cow<'static, [u8]>, String)> {
        // Normalize path (remove leading slash)
        let normalized = path.trim_start_matches('/');

        // Try exact match first
        if let Some(content) = Self::get(normalized) {
            let mime = mime_guess::from_path(normalized)
                .first_or_octet_stream()
                .to_string();
            return Some((content.data, mime));
        }

        // For SPA routing: if no file found and not an API route, return index.html
        if !is_api_path(path) {
            if let Some(content) = Self::get("index.html") {
                return Some((content.data, "text/html; charset=utf-8".to_string()));
            }
        }

        None
    }
}

/// The `/events` stream is served by the API itself; it never falls back to
/// `index.html` or goes through the dev proxy (which buffers responses).
fn is_events_path(path: &str) -> bool {
    path == "/events" || path.starts_with("/events/")
}

/// Paths owned by the API, which never fall back to `index.html`.
fn is_api_path(path: &str) -> bool {
    path.starts_with("/contracts") || is_events_path(path)
}

/// Nothing is served under `/api`: the endpoints sit at the root. In dev mode
/// such a path must not go to Trunk, because a Trunk rule proxying `/api/`
/// back here would have the two servers hand the request to each other until
/// one gives up.
fn is_unserved_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

/// Paths that are answered here in dev mode, never handed to Trunk.
fn is_kept_from_dev_proxy(path: &str) -> bool {
    is_events_path(path) || is_unserved_api_path(path)
}

/// Handler for serving static files or proxying to Trunk dev server
pub async fn serve_static_or_proxy(uri: Uri, dev_mode: bool, dev_server_url: &str) -> Response {
    if dev_mode && is_kept_from_dev_proxy(uri.path()) {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap();
    }

    if dev_mode {
        // Proxy to Trunk dev server
        proxy_to_dev_server(&uri, dev_server_url).await
    } else {
        // Serve embedded static files
        serve_embedded_static(&uri).await
    }
}

/// Serve embedded static files
async fn serve_embedded_static(uri: &Uri) -> Response {
    let path = uri.path();

    match StaticAssets::get_with_fallback(path) {
        Some((content, mime_type)) => {
            // Determine cache policy
            let cache_header = if path == "/" || path.ends_with("index.html") {
                // No caching for HTML (SPA entrypoint)
                "no-cache, no-store, must-revalidate"
            } else if path.ends_with(".wasm") || path.ends_with(".js") {
                // Aggressive caching for hashed Trunk assets (WASM/JS)
                "public, max-age=31536000, immutable"
            } else if path.starts_with("/snippets/") {
                // Aggressive caching for JS snippets (wasm-bindgen generated)
                "public, max-age=31536000, immutable"
            } else {
                // Moderate caching for other assets (CSS, images)
                "public, max-age=3600"
            };

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CACHE_CONTROL, cache_header)
                .body(Body::from(content.into_owned()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap(),
    }
}

/// Proxy request to Trunk dev server
async fn proxy_to_dev_server(uri: &Uri, dev_server_url: &str) -> Response {
    let client = reqwest::Client::new();
    let url = format!("{}{}", dev_server_url, uri);

    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await.unwrap_or_default();

            let mut response = Response::builder().status(status);

            // Copy relevant headers (skip connection-specific headers)
            for (key, value) in headers.iter() {
                let key_str = key.as_str();
                if !matches!(
                    key_str,
                    "connection" | "transfer-encoding" | "content-length"
                ) {
                    response = response.header(key, value);
                }
            }

            response.body(Body::from(body)).unwrap()
        }
        Err(e) => {
            tracing::error!("Failed to proxy to dev server: {}", e);
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Dev server unreachable: {}", e)))
                .unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_and_events_paths_are_kept_from_the_dev_proxy() {
        for path in ["/api", "/api/", "/api/rooms/x", "/events", "/events/x"] {
            assert!(is_kept_from_dev_proxy(path), "{}", path);
        }
        // the app itself, and names that merely start the same way
        for path in ["/", "/index.html", "/apiary", "/eventsource.js", "/contracts/a.yaml"] {
            assert!(!is_kept_from_dev_proxy(path), "{}", path);
        }
    }

    #[tokio::test]
    async fn an_api_path_in_dev_mode_is_not_found_without_asking_the_dev_server() {
        // Nothing listens here; reaching for it would be a 502, not a 404.
        let uri: Uri = "/api/anything".parse().unwrap();
        let response = serve_static_or_proxy(uri, true, "http://127.0.0.1:9").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
