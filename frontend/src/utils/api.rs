use gloo_net::http::Request;

/// Get the API base URL
fn get_api_base_url() -> String {
    // In development via Trunk, we proxy through the dev server
    // In production, we're served from the same origin as the API
    String::new() // Empty string means same origin
}

/// Fetch contract content from the server API
pub async fn fetch_server_contract(contract_path: &str) -> Result<Option<String>, String> {
    if contract_path.trim().is_empty() {
        return Ok(None);
    }

    let base_url = get_api_base_url();
    // URL encode each path segment
    let encoded_path = contract_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            js_sys::encode_uri_component(segment)
                .as_string()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("/");

    let url = format!("{}/contracts/{}", base_url, encoded_path);

    match Request::get(&url)
        .header("Accept", "application/x-yaml")
        .send()
        .await
    {
        Ok(response) => {
            if response.status() == 404 {
                Ok(None) // Contract not found on server
            } else if response.ok() {
                match response.text().await {
                    Ok(text) => Ok(Some(text)),
                    Err(e) => Err(format!("Failed to read response: {}", e)),
                }
            } else {
                Err(format!(
                    "Failed to fetch contract: {} {}",
                    response.status(),
                    response.status_text()
                ))
            }
        }
        Err(e) => Err(format!("Network error: {}", e)),
    }
}

/// Push contract content to the server API
pub async fn push_contract_to_server(contract_path: &str, content: &str) -> Result<(), String> {
    let base_url = get_api_base_url();
    // URL encode each path segment
    let encoded_path = contract_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            js_sys::encode_uri_component(segment)
                .as_string()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("/");

    let url = format!("{}/contracts/{}", base_url, encoded_path);

    match Request::put(&url)
        .header("Content-Type", "application/x-yaml")
        .body(content)
        .map_err(|e| format!("Failed to create request: {}", e))?
        .send()
        .await
    {
        Ok(response) => {
            if response.ok() {
                Ok(())
            } else {
                Err(format!(
                    "Failed to push contract: {} {}",
                    response.status(),
                    response.status_text()
                ))
            }
        }
        Err(e) => Err(format!("Network error: {}", e)),
    }
}
