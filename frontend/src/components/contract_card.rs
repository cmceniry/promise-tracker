use leptos::prelude::*;
use serde::Deserialize;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, DragEvent, MouseEvent, Url};

/// Diff status for a contract comparing local vs server
#[derive(Clone, Debug, Default)]
pub struct DiffStatus {
    pub is_different: bool,
    pub is_loading: bool,
    #[allow(dead_code)] // Will be used in Phase 6 for diff error display
    pub error: Option<String>,
}

/// Extract agents and superagents from contract YAML content
///
/// Parses the YAML content (potentially multiple documents) and extracts
/// entities based on their `kind` field:
/// - "Agent" or "Component" (or no kind) -> agents
/// - "SuperAgent" or "Collective" -> superagents
pub fn extract_agents_and_superagents(contract_text: &str) -> (Vec<String>, Vec<String>) {
    let mut agents = Vec::new();
    let mut superagents = Vec::new();

    if contract_text.trim().is_empty() {
        return (agents, superagents);
    }

    // Parse multi-document YAML
    for document in serde_yaml::Deserializer::from_str(contract_text) {
        let value: Result<serde_yaml::Value, _> = Deserialize::deserialize(document);
        if let Ok(doc) = value {
            if let serde_yaml::Value::Mapping(map) = doc {
                let kind = map
                    .get(&serde_yaml::Value::String("kind".to_string()))
                    .and_then(|v| v.as_str());
                let name = map
                    .get(&serde_yaml::Value::String("name".to_string()))
                    .and_then(|v| v.as_str());

                if let Some(name) = name {
                    match kind {
                        Some("SuperAgent") | Some("Collective") => {
                            superagents.push(name.to_string());
                        }
                        Some("Agent") | Some("Component") | None => {
                            agents.push(name.to_string());
                        }
                        _ => {
                            // Unknown kind, treat as agent
                            agents.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    (agents, superagents)
}

/// Create a download URL for the given content
fn create_download_url(content: &str) -> Option<String> {
    let array = js_sys::Array::new();
    array.push(&JsValue::from_str(content));

    let options = BlobPropertyBag::new();
    options.set_type("text/yaml");

    Blob::new_with_str_sequence_and_options(&array, &options)
        .ok()
        .and_then(|blob| Url::create_object_url_with_blob(&blob).ok())
}

/// Revoke a previously created download URL
fn revoke_download_url(url: &str) {
    let _ = Url::revoke_object_url(url);
}

/// Draggable card displaying individual contract with metadata.
#[component]
pub fn ContractCard(
    contract_id: String,
    #[prop(into)] contract_filename: Signal<String>,
    #[prop(into)] contract_content: Signal<String>,
    #[prop(into)] contract_error: Signal<String>,
    #[prop(into)] contract_sims: Signal<HashSet<String>>,
    #[prop(into)] on_delete: Callback<String>,
    #[prop(into)] on_toggle_sim: Callback<(String, String)>,
    #[prop(into)] on_edit: Callback<String>,
    #[prop(into)] simulations: Signal<Vec<String>>,
    card_class_name: String,
    diff_status: DiffStatus,
) -> impl IntoView {
    // Extract agents and superagents reactively
    let agents_and_superagents = Memo::new(move |_| {
        extract_agents_and_superagents(&contract_content.get())
    });
    let agents = Memo::new(move |_| agents_and_superagents.get().0);
    let superagents = Memo::new(move |_| agents_and_superagents.get().1);

    // Create download URL reactively
    let download_url = Memo::new(move |_| create_download_url(&contract_content.get()));

    // Clean up URL when component unmounts
    on_cleanup(move || {
        if let Some(url) = download_url.get() {
            revoke_download_url(&url);
        }
    });

    // Track if we're dragging
    let (is_dragging, set_is_dragging) = signal(false);

    // Clone IDs for closures
    let id_for_delete = contract_id.clone();
    let id_for_edit = contract_id.clone();
    let id_for_dragstart = contract_id.clone();

    // Handle card click for editing
    let handle_card_click = move |ev: MouseEvent| {
        // Get the target element
        if let Some(target) = ev.target() {
            if let Ok(element) = target.dyn_into::<web_sys::Element>() {
                // Check if clicked on a button, link, or drag handle
                if element.closest("button").ok().flatten().is_some()
                    || element.closest("a").ok().flatten().is_some()
                    || element.closest(".drag-handle").ok().flatten().is_some()
                {
                    return;
                }
            }
        }
        on_edit.run(id_for_edit.clone());
    };

    // Handle delete button
    let handle_delete = move |ev: MouseEvent| {
        ev.stop_propagation();
        on_delete.run(id_for_delete.clone());
    };

    // Drag and drop handlers
    let handle_dragstart = move |ev: DragEvent| {
        set_is_dragging.set(true);
        if let Some(data_transfer) = ev.data_transfer() {
            let _ = data_transfer.set_data("text/plain", &id_for_dragstart);
            data_transfer.set_effect_allowed("move");
        }
    };

    let handle_dragend = move |_: DragEvent| {
        set_is_dragging.set(false);
    };

    // Determine card border style based on diff status
    let card_border_style = if diff_status.is_different {
        "border: 3px solid #ffc107; background-color: rgba(255, 193, 7, 0.1);"
    } else {
        ""
    };

    view! {
        <div
            class=format!("card mb-2 {}", card_class_name)
            style=move || format!(
                "cursor: pointer; position: relative; {}{}",
                card_border_style,
                if is_dragging.get() { "opacity: 0.5;" } else { "" }
            )
            draggable="true"
            on:click=handle_card_click
            on:dragstart=handle_dragstart
            on:dragend=handle_dragend
        >
            <div class="card-body p-2">
                // Drag handle
                <div
                    class="drag-handle"
                    style="position: absolute; left: 8px; top: 50%; transform: translateY(-50%); cursor: grab; padding: 4px; display: flex; align-items: center; z-index: 1; color: #6c757d;"
                >
                    // Grip icon (using Unicode character as fallback)
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" fill="currentColor" viewBox="0 0 16 16">
                        <path d="M7 2a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 5a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 8a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm-3 3a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm-3 3a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0z"/>
                    </svg>
                </div>

                // Filename and diff status
                <div style="margin-bottom: 0.5rem; margin-left: 32px; display: flex; align-items: center; gap: 0.5rem;">
                    <strong>
                        {move || {
                            let filename = contract_filename.get();
                            if filename.is_empty() {
                                "untitled-contract.yaml".to_string()
                            } else {
                                filename
                            }
                        }}
                    </strong>

                    // Loading spinner for diff check
                    {if diff_status.is_loading {
                        view! {
                            <div class="spinner-border spinner-border-sm text-secondary" role="status">
                                <span class="visually-hidden">"Loading..."</span>
                            </div>
                        }.into_any()
                    } else if diff_status.is_different {
                        view! {
                            <span class="badge bg-warning d-flex align-items-center gap-1">
                                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" fill="currentColor" viewBox="0 0 16 16">
                                    <path d="M8.982 1.566a1.13 1.13 0 0 0-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 5.995A.905.905 0 0 1 8 5zm.002 6a1 1 0 1 1 0 2 1 1 0 0 1 0-2z"/>
                                </svg>
                                "Diff"
                            </span>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}

                    // Validation status badge
                    {move || {
                        let error = contract_error.get();
                        if !error.is_empty() {
                            view! {
                                <span class="badge bg-danger" title=error>"Error"</span>
                            }.into_any()
                        } else {
                            view! {
                                <span class="badge bg-success">"Valid"</span>
                            }.into_any()
                        }
                    }}
                </div>

                // Agents and Superagents badges
                {move || {
                    let agents_list = agents.get();
                    let superagents_list = superagents.get();

                    if !agents_list.is_empty() || !superagents_list.is_empty() {
                        view! {
                            <div style="margin-bottom: 0.5rem; margin-left: 32px; font-size: 0.9em;">
                                {if !agents_list.is_empty() {
                                    view! {
                                        <div style="margin-bottom: 0.25rem;">
                                            <strong>"Agents: "</strong>
                                            {agents_list.iter().map(|agent| {
                                                view! {
                                                    <span class="badge bg-primary me-1">{agent.clone()}</span>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                                {if !superagents_list.is_empty() {
                                    view! {
                                        <div>
                                            <strong>"Superagents: "</strong>
                                            {superagents_list.iter().map(|superagent| {
                                                view! {
                                                    <span class="badge bg-info me-1">{superagent.clone()}</span>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}

                // Simulation buttons and action buttons
                <div style="margin-bottom: 0.5rem; margin-left: 32px; display: flex; align-items: center; justify-content: space-between;">
                    // Simulation toggle buttons
                    <div style="display: flex; gap: 0.25rem;">
                        {move || simulations.get().iter().map(|sim| {
                            let sim_clone = sim.clone();
                            let sim_for_class = sim.clone();
                            let contract_id_clone = contract_id.clone();
                            let on_toggle = on_toggle_sim.clone();

                            view! {
                                <button
                                    type="button"
                                    class=move || format!("btn btn-sm {}", if contract_sims.get().contains(&sim_for_class) { "btn-success" } else { "btn-danger" })
                                    on:click=move |ev: MouseEvent| {
                                        ev.stop_propagation();
                                        on_toggle.run((contract_id_clone.clone(), sim_clone.clone()));
                                    }
                                >
                                    {sim.clone()}
                                </button>
                            }
                        }).collect_view()}
                    </div>

                    // Action buttons (download and delete)
                    <div style="display: flex; gap: 0.25rem;">
                        // Download button
                        {move || {
                            if let Some(url) = download_url.get() {
                                let filename = contract_filename.get();
                                view! {
                                    <a
                                        download=filename
                                        href=url
                                        on:click=move |ev: MouseEvent| {
                                            ev.stop_propagation();
                                        }
                                    >
                                        <button type="button" class="btn btn-sm btn-primary" aria-label="Download">
                                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                                <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/>
                                                <path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/>
                                            </svg>
                                        </button>
                                    </a>
                                }.into_any()
                            } else {
                                view! {
                                    <button type="button" class="btn btn-sm btn-primary" disabled aria-label="Download">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                            <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/>
                                            <path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/>
                                        </svg>
                                    </button>
                                }.into_any()
                            }
                        }}

                        // Delete button
                        <button
                            type="button"
                            class="btn btn-sm btn-danger"
                            on:click=handle_delete
                            aria-label="Delete"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                <path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6z"/>
                                <path fill-rule="evenodd" d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1v1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/>
                            </svg>
                        </button>
                    </div>
                </div>

                // Error message
                {move || {
                    let error = contract_error.get();
                    if !error.is_empty() {
                        view! {
                            <div class="alert alert-danger py-1 px-2 mb-0" style="margin-left: 32px; font-size: 0.85em;">
                                {error}
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
