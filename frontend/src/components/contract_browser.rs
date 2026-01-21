use gloo_net::http::Request;
use leptos::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

/// Directory entry from the API
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
}

/// Encode a path for use in API URLs
fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| js_sys::encode_uri_component(segment).as_string().unwrap())
        .collect::<Vec<_>>()
        .join("/")
}

/// Fetch directory listing from API
async fn fetch_directory_listing(path: Option<&str>) -> Result<Vec<DirectoryEntry>, String> {
    let url = match path {
        Some(p) if !p.is_empty() => format!("/contracts/{}", encode_path(p)),
        _ => "/contracts".to_string(),
    };

    let response = Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!(
            "Failed to fetch directory: {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Vec<DirectoryEntry>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch contract content from API
async fn fetch_contract(contract_id: &str) -> Result<String, String> {
    let url = format!("/contracts/{}", encode_path(contract_id));

    let response = Request::get(&url)
        .header("Accept", "application/x-yaml")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!(
            "Failed to fetch contract: {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Read error: {}", e))
}

/// Tree browser modal for loading contracts from API server.
#[component]
pub fn ContractBrowser(
    show: ReadSignal<bool>,
    #[prop(into)] on_hide: Callback<()>,
    #[prop(into)] on_select_contract: Callback<(String, String, String)>, // (id, filename, content)
    #[prop(into)] downloaded_contract_paths: Signal<HashSet<String>>,
) -> impl IntoView {
    // State signals
    let (expanded_paths, set_expanded_paths) = signal::<HashSet<String>>(HashSet::new());
    let (loaded_children, set_loaded_children) =
        signal::<HashMap<String, Vec<DirectoryEntry>>>(HashMap::new());
    let (loading_paths, set_loading_paths) = signal::<HashSet<String>>(HashSet::new());
    let (root_entries, set_root_entries) = signal::<Vec<DirectoryEntry>>(Vec::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let (downloading_contracts, set_downloading_contracts) =
        signal::<HashSet<String>>(HashSet::new());
    let (selected_contracts, set_selected_contracts) = signal::<HashSet<String>>(HashSet::new());

    // Ref for focusing the modal
    let modal_ref = NodeRef::<leptos::html::Div>::new();

    // Use the signal for downloaded paths
    let downloaded_paths = downloaded_contract_paths;

    let is_visible = move || show.get();

    // Load root directory when modal opens
    Effect::new(move |_| {
        if show.get() {
            // Reset state
            set_expanded_paths.set(HashSet::new());
            set_loaded_children.set(HashMap::new());
            set_loading_paths.set(HashSet::new());
            set_downloading_contracts.set(HashSet::new());
            set_selected_contracts.set(HashSet::new());
            set_error.set(None);

            // Load root directory
            set_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_directory_listing(None).await {
                    Ok(entries) => {
                        set_root_entries.set(entries);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_root_entries.set(Vec::new());
                    }
                }
                set_loading.set(false);
            });

            // Focus the modal for keyboard events
            if let Some(el) = modal_ref.get() {
                let _ = el.unchecked_ref::<HtmlElement>().focus();
            }
        }
    });

    // Handle close
    let handle_close = move |_| {
        set_expanded_paths.set(HashSet::new());
        set_loaded_children.set(HashMap::new());
        set_loading_paths.set(HashSet::new());
        set_root_entries.set(Vec::new());
        set_downloading_contracts.set(HashSet::new());
        set_selected_contracts.set(HashSet::new());
        set_error.set(None);
        on_hide.run(());
    };

    // Toggle directory expansion
    let toggle_expand = move |path: String| {
        let is_expanded = expanded_paths.get().contains(&path);

        if is_expanded {
            // Collapse
            set_expanded_paths.update(|paths| {
                paths.remove(&path);
            });
        } else {
            // Expand - load children if not already loaded
            set_expanded_paths.update(|paths| {
                paths.insert(path.clone());
            });

            // Check if already loaded
            if !loaded_children.get().contains_key(&path) {
                // Mark as loading
                set_loading_paths.update(|paths| {
                    paths.insert(path.clone());
                });

                let path_clone = path.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_directory_listing(Some(&path_clone)).await {
                        Ok(entries) => {
                            set_loaded_children.update(|children| {
                                children.insert(path_clone.clone(), entries);
                            });
                        }
                        Err(e) => {
                            set_error.set(Some(e));
                        }
                    }
                    set_loading_paths.update(|paths| {
                        paths.remove(&path_clone);
                    });
                });
            }
        }
    };

    // Download a contract
    let download_contract = move |path: String, _name: String| {
        // Check if already downloading or downloaded
        if downloading_contracts.get().contains(&path) || downloaded_paths.get().contains(&path) {
            return;
        }

        // Mark as downloading
        set_downloading_contracts.update(|paths| {
            paths.insert(path.clone());
        });
        set_error.set(None);

        let path_clone = path.clone();
        let on_select = on_select_contract.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_contract(&path_clone).await {
                Ok(content) => {
                    // Use full path as filename to match subsequent server calls
                    on_select.run((path_clone.clone(), path_clone.clone(), content));
                }
                Err(e) => {
                    set_error.update(|err| {
                        let msg = format!("{}: {}", path_clone, e);
                        *err = Some(match err.take() {
                            Some(prev) => format!("{}; {}", prev, msg),
                            None => msg,
                        });
                    });
                }
            }
            set_downloading_contracts.update(|paths| {
                paths.remove(&path_clone);
            });
        });
    };

    // Dismiss error
    let dismiss_error = move |_| {
        set_error.set(None);
    };

    // Handle adding selected contracts
    let handle_add_selected = move |_| {
        let contracts_to_download: Vec<String> = selected_contracts
            .get()
            .into_iter()
            .filter(|path| {
                !downloaded_paths.get().contains(path) &&
                !downloading_contracts.get().contains(path)
            })
            .collect();

        if contracts_to_download.is_empty() {
            return;
        }

        // Mark all as downloading
        set_downloading_contracts.update(|paths| {
            paths.extend(contracts_to_download.iter().cloned());
        });

        // Clear selections
        set_selected_contracts.set(HashSet::new());

        // Download each contract in parallel
        let on_select = on_select_contract.clone();
        for contract_path in contracts_to_download {
            let path_clone = contract_path.clone();
            let on_select_clone = on_select.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match fetch_contract(&path_clone).await {
                    Ok(content) => {
                        on_select_clone.run((
                            path_clone.clone(),
                            path_clone.clone(),
                            content
                        ));
                    }
                    Err(e) => {
                        set_error.update(|err| {
                            let msg = format!("{}: {}", path_clone, e);
                            *err = Some(match err.take() {
                                Some(prev) => format!("{}; {}", prev, msg),
                                None => msg,
                            });
                        });
                    }
                }

                set_downloading_contracts.update(|paths| {
                    paths.remove(&path_clone);
                });
            });
        }
    };

    // Handle escape key to close modal
    let handle_keydown = {
        let on_hide = on_hide.clone();
        move |ev: KeyboardEvent| {
            if ev.key() == "Escape" {
                ev.prevent_default();
                set_expanded_paths.set(HashSet::new());
                set_loaded_children.set(HashMap::new());
                set_loading_paths.set(HashSet::new());
                set_root_entries.set(Vec::new());
                set_downloading_contracts.set(HashSet::new());
                set_selected_contracts.set(HashSet::new());
                set_error.set(None);
                on_hide.run(());
            }
        }
    };

    view! {
        <div
            class="modal"
            class:show=is_visible
            style:display=move || if is_visible() { "block" } else { "none" }
            tabindex="-1"
            on:keydown=handle_keydown
            node_ref=modal_ref
        >
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">"Load Contract from Server"</h5>
                        <button
                            type="button"
                            class="btn-close"
                            on:click=handle_close
                        ></button>
                    </div>
                    <div class="modal-body" style="max-height: 60vh; overflow-y: auto;">
                        // Error alert
                        <Show when=move || error.get().is_some()>
                            <div class="alert alert-danger alert-dismissible fade show" role="alert">
                                {move || error.get().unwrap_or_default()}
                                <button
                                    type="button"
                                    class="btn-close"
                                    on:click=dismiss_error
                                ></button>
                            </div>
                        </Show>

                        // Loading spinner
                        <Show when=move || loading.get()>
                            <div style="text-align: center; padding: 2rem;">
                                <div class="spinner-border" role="status">
                                    <span class="visually-hidden">"Loading..."</span>
                                </div>
                            </div>
                        </Show>

                        // Tree view
                        <Show when=move || !loading.get()>
                            <ul class="list-group">
                                {move || {
                                    let entries = root_entries.get();
                                    if entries.is_empty() {
                                        view! {
                                            <li class="list-group-item">
                                                "No contracts or directories found."
                                            </li>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <For
                                                each=move || root_entries.get()
                                                key=|entry| entry.name.clone()
                                                children=move |entry| {
                                                    let path = entry.name.clone();
                                                    view! {
                                                        <TreeNode
                                                            entry=entry
                                                            path=path
                                                            level=0
                                                            expanded_paths=expanded_paths
                                                            loaded_children=loaded_children
                                                            loading_paths=loading_paths
                                                            on_toggle_expand=toggle_expand.clone()
                                                            on_download_contract=download_contract.clone()
                                                            downloaded_paths=downloaded_paths
                                                            downloading_contracts=downloading_contracts
                                                            selected_contracts=selected_contracts
                                                            set_selected_contracts=set_selected_contracts
                                                        />
                                                    }
                                                }
                                            />
                                        }.into_any()
                                    }
                                }}
                            </ul>
                        </Show>
                    </div>
                    <div class="modal-footer">
                        <button
                            type="button"
                            class="btn btn-primary"
                            disabled=move || selected_contracts.get().is_empty() ||
                                            !downloading_contracts.get().is_empty()
                            on:click=handle_add_selected
                        >
                            {move || {
                                let count = selected_contracts.get().len();
                                if count > 0 {
                                    format!("Add Selected ({})", count)
                                } else {
                                    "Add Selected".to_string()
                                }
                            }}
                        </button>
                        <button
                            type="button"
                            class="btn btn-secondary"
                            on:click=handle_close
                        >
                            "Close"
                        </button>
                    </div>
                </div>
            </div>
        </div>
        // Modal backdrop
        <Show when=is_visible>
            <div class="modal-backdrop fade show"></div>
        </Show>
    }
}

/// Recursive tree node component
#[component]
fn TreeNode(
    entry: DirectoryEntry,
    path: String,
    level: usize,
    expanded_paths: ReadSignal<HashSet<String>>,
    loaded_children: ReadSignal<HashMap<String, Vec<DirectoryEntry>>>,
    loading_paths: ReadSignal<HashSet<String>>,
    on_toggle_expand: impl Fn(String) + Clone + Send + Sync + 'static,
    on_download_contract: impl Fn(String, String) + Clone + Send + Sync + 'static,
    downloaded_paths: Signal<HashSet<String>>,
    downloading_contracts: ReadSignal<HashSet<String>>,
    selected_contracts: ReadSignal<HashSet<String>>,
    set_selected_contracts: WriteSignal<HashSet<String>>,
) -> impl IntoView {
    let is_directory = entry.entry_type == "directory";
    let entry_name = entry.name.clone();

    let indent = level * 20;

    // Store path for reactive lookups
    let stored_path = StoredValue::new(path.clone());

    // Create reactive derived signals using stored path
    let is_expanded = move || expanded_paths.get().contains(&stored_path.get_value());
    let is_loading = move || loading_paths.get().contains(&stored_path.get_value());
    let is_downloaded = move || downloaded_paths.get().contains(&stored_path.get_value());
    let is_downloading = move || {
        downloading_contracts
            .get()
            .contains(&stored_path.get_value())
    };

    // Store callbacks for reuse
    let stored_toggle = StoredValue::new(on_toggle_expand.clone());
    let stored_download = StoredValue::new(on_download_contract.clone());

    let handle_directory_click = {
        let p = path.clone();
        move |_| {
            stored_toggle.get_value()(p.clone());
        }
    };

    let handle_checkbox_toggle = {
        let p = path.clone();
        move |ev: web_sys::MouseEvent| {
            ev.stop_propagation(); // Prevent directory expansion
            let contract_path = p.clone();
            set_selected_contracts.update(|selected| {
                if selected.contains(&contract_path) {
                    selected.remove(&contract_path);
                } else {
                    if !downloaded_paths.get().contains(&contract_path) {
                        selected.insert(contract_path);
                    }
                }
            });
        }
    };

    view! {
        <li
            class="list-group-item"
            class:list-group-item-action=is_directory
            style:cursor=move || if is_directory { "pointer" } else { "default" }
            style:display="flex"
            style:align-items="center"
            style:gap="0.5rem"
            style:padding-left=format!("{}px", 8 + indent)
            style:opacity=move || if is_downloaded() { "0.6" } else { "1" }
            on:click=move |_| {
                if is_directory {
                    handle_directory_click(());
                }
            }
        >
            // Directory chevron and icon
            {if is_directory {
                view! {
                    <>
                        <span style="flex-shrink: 0; width: 16px;">
                            {move || if is_expanded() { "▼" } else { "▶" }}
                        </span>
                        <span style="flex-shrink: 0;">"📁"</span>
                    </>
                }.into_any()
            } else {
                view! {
                    <span style="flex-shrink: 0;">"📄"</span>
                }.into_any()
            }}

            // Entry name
            <span style:text-decoration=move || if is_downloaded() { "line-through" } else { "none" }
                  style:flex-grow="1">
                {entry_name.clone()}
            </span>

            // Checkbox for contracts
            {if !is_directory {
                view! {
                    <input
                        type="checkbox"
                        class="form-check-input"
                        style:cursor="pointer"
                        checked=move || selected_contracts.get().contains(&stored_path.get_value())
                        disabled=move || is_downloaded() || is_downloading()
                        on:click=handle_checkbox_toggle
                    />
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            // Loading spinner
            <Show when=move || is_loading() || is_downloading()>
                <span class="spinner-border spinner-border-sm" style="margin-left: 0.5rem;"></span>
            </Show>
        </li>

        // Children (for expanded directories)
        <Show when=move || is_directory && is_expanded() && !is_loading()>
            {move || {
                let cp = stored_path.get_value();
                let children = loaded_children.get()
                    .get(&cp)
                    .cloned()
                    .unwrap_or_default();

                if children.is_empty() {
                    view! {
                        <li
                            class="list-group-item"
                            style:padding-left=format!("{}px", 8 + indent + 20)
                            style:font-style="italic"
                            style:color="#666"
                        >
                            "Empty directory"
                        </li>
                    }.into_any()
                } else {
                    let toggle = stored_toggle.get_value();
                    let download = stored_download.get_value();
                    let parent_path = cp.clone();

                    view! {
                        <For
                            each=move || {
                                let pp = stored_path.get_value();
                                loaded_children.get()
                                    .get(&pp)
                                    .cloned()
                                    .unwrap_or_default()
                            }
                            key=|entry| entry.name.clone()
                            children={
                                let toggle = toggle.clone();
                                let download = download.clone();
                                let parent = parent_path.clone();
                                move |child| {
                                    let child_path = format!("{}/{}", parent, child.name);
                                    view! {
                                        <TreeNode
                                            entry=child
                                            path=child_path
                                            level=level + 1
                                            expanded_paths=expanded_paths
                                            loaded_children=loaded_children
                                            loading_paths=loading_paths
                                            on_toggle_expand=toggle.clone()
                                            on_download_contract=download.clone()
                                            downloaded_paths=downloaded_paths
                                            downloading_contracts=downloading_contracts
                                            selected_contracts=selected_contracts
                                            set_selected_contracts=set_selected_contracts
                                        />
                                    }
                                }
                            }
                        />
                    }.into_any()
                }
            }}
        </Show>
    }
}
