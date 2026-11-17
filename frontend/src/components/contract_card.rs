use leptos::prelude::*;
use serde::Deserialize;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, DragEvent, MouseEvent, Url};

use crate::follow::CardState;

/// Diff status for a contract comparing local vs server
#[derive(Clone, Debug, Default)]
pub struct DiffStatus {
    pub is_different: bool,
    pub is_loading: bool,
    #[allow(dead_code)] // Will be used in Phase 6 for diff error display
    pub error: Option<String>,
}

/// How a followed card reads: its status word, tooltip, flash counter, and
/// the error of a pending invalid server copy. None on an unfollowed card.
#[derive(Clone, Debug, PartialEq)]
pub struct FollowView {
    pub state: CardState,
    pub tooltip: String,
    /// Bumped each time an update lands; the card flashes on a change.
    pub flash: u32,
    /// Set while the server copy is invalid and the card shows the last good.
    pub pending_err: Option<String>,
}

/// The Bootstrap Icons `bi-broadcast` glyph, for the follow button.
pub(crate) fn broadcast_icon() -> impl IntoView {
    view! {
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
            <path d="M3.05 3.05a7 7 0 0 0 0 9.9.5.5 0 0 1-.707.707 8 8 0 0 1 0-11.314.5.5 0 0 1 .707.707m2.122 2.122a4 4 0 0 0 0 5.656.5.5 0 1 1-.708.708 5 5 0 0 1 0-7.072.5.5 0 0 1 .708.708m5.656-.708a.5.5 0 0 1 .708 0 5 5 0 0 1 0 7.072.5.5 0 1 1-.708-.708 4 4 0 0 0 0-5.656.5.5 0 0 1 0-.708m2.122-2.12a.5.5 0 0 1 .707 0 8 8 0 0 1 0 11.313.5.5 0 0 1-.707-.707 7 7 0 0 0 0-9.9.5.5 0 0 1 0-.707zM10 8a2 2 0 1 1-4 0 2 2 0 0 1 4 0"/>
        </svg>
    }
}

/// Extract agents, superagents and instances from contract YAML content
///
/// Parses the YAML content (potentially multiple documents) and extracts
/// entities based on their `kind` field:
/// - "Agent" or "Component" (or no kind) -> agents
/// - "SuperAgent" or "Collective" -> superagents
/// - "Instance" -> instances
pub fn extract_agents_and_superagents(
    contract_text: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut agents = Vec::new();
    let mut superagents = Vec::new();
    let mut instances = Vec::new();

    if contract_text.trim().is_empty() {
        return (agents, superagents, instances);
    }

    // Parse multi-document YAML. Stop at the first error: after a syntax
    // error serde_yaml's document iterator yields that same error forever.
    for document in serde_yaml::Deserializer::from_str(contract_text) {
        let value: Result<serde_yaml::Value, _> = Deserialize::deserialize(document);
        let Ok(doc) = value else { break };
        let serde_yaml::Value::Mapping(map) = doc else {
            continue;
        };
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
                Some("Instance") => {
                    instances.push(name.to_string());
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

    (agents, superagents, instances)
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
    #[prop(into)] on_push: Callback<String>,
    #[prop(into)] simulations: Signal<Vec<String>>,
    card_class_name: String,
    #[prop(into)] diff_status: Signal<DiffStatus>,
    /// Where the card came from on the server; the follow button needs one
    #[prop(into)]
    contract_server_path: Signal<Option<String>>,
    /// Follow status, None when the card is not following
    #[prop(into)]
    follow_view: Signal<Option<FollowView>>,
    #[prop(into)] on_toggle_follow: Callback<String>,
) -> impl IntoView {
    let is_following = move || follow_view.with(|f| f.is_some());
    // Extract agents and superagents reactively
    let agents_and_superagents =
        Memo::new(move |_| extract_agents_and_superagents(&contract_content.get()));
    let agents = Memo::new(move |_| agents_and_superagents.get().0);
    let superagents = Memo::new(move |_| agents_and_superagents.get().1);
    let instances = Memo::new(move |_| agents_and_superagents.get().2);

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
    let id_for_push = contract_id.clone();
    let id_for_follow = contract_id.clone();
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

    view! {
        <div
            class=move || {
                // Alternating between two identical animations restarts the
                // flash on every update that lands.
                let flash = match follow_view.with(|f| f.as_ref().map(|f| f.flash)) {
                    Some(n) if n > 0 => if n % 2 == 0 { " follow-flash-a" } else { " follow-flash-b" },
                    _ => "",
                };
                format!("card mb-2 {}{}", card_class_name, flash)
            }
            style=move || format!(
                "cursor: pointer; position: relative; {}{}",
                if is_following() {
                    "border-left: 4px solid #0dcaf0;"
                } else if diff_status.get().is_different {
                    "border: 3px solid #ffc107; background-color: rgba(255, 193, 7, 0.1);"
                } else { "" },
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

                    // Following status, or the loading spinner / diff badge
                    {move || {
                        let ds = diff_status.get();
                        if let Some(fv) = follow_view.get() {
                            let class = match fv.state {
                                CardState::Live => "badge bg-info text-dark",
                                CardState::Syncing | CardState::Stale => "badge bg-secondary",
                                CardState::Error => "badge bg-danger",
                                CardState::Missing => "badge bg-warning text-dark",
                            };
                            view! {
                                <span class=class title=fv.tooltip>
                                    "following · " {fv.state.label()}
                                </span>
                            }.into_any()
                        } else if ds.is_loading {
                            view! {
                                <div class="spinner-border spinner-border-sm text-secondary" role="status">
                                    <span class="visually-hidden">"Loading..."</span>
                                </div>
                            }.into_any()
                        } else if ds.is_different {
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
                        }
                    }}

                    // Validation status badge
                    {move || {
                        let error = contract_error.get();
                        let pending = follow_view.with(|f| f.as_ref().and_then(|f| f.pending_err.clone()));
                        if let Some(pending) = pending {
                            view! {
                                <span class="badge bg-danger" title=pending>"Error"</span>
                            }.into_any()
                        } else if !error.is_empty() {
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

                // Agents, Superagents and Instances badges
                {move || {
                    let agents_list = agents.get();
                    let superagents_list = superagents.get();
                    let instances_list = instances.get();

                    if !agents_list.is_empty()
                        || !superagents_list.is_empty()
                        || !instances_list.is_empty()
                    {
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
                                        <div style="margin-bottom: 0.25rem;">
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
                                {if !instances_list.is_empty() {
                                    view! {
                                        <div>
                                            <strong>"Instances: "</strong>
                                            {instances_list.iter().map(|instance| {
                                                view! {
                                                    <span class="badge bg-secondary me-1">{instance.clone()}</span>
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
                    <div style="display: flex; gap: 0.25rem; align-items: center;">
                        // Push to server button (shown when local differs from server)
                        {
                            let id_for_push_inner = id_for_push.clone();
                            move || {
                                let id_push = id_for_push_inner.clone();
                                if diff_status.get().is_different && !is_following() {
                                    view! {
                                        <button
                                            type="button"
                                            class="btn btn-sm btn-warning"
                                            on:click=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                on_push.run(id_push.clone());
                                            }
                                            aria-label="Push to server"
                                            title="Push to server"
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                                <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/>
                                                <path d="M7.646 1.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1-.708.708L8.5 2.707V11.5a.5.5 0 0 1-1 0V2.707L5.354 4.854a.5.5 0 1 1-.708-.708l3-3z"/>
                                            </svg>
                                        </button>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }
                            }
                        }

                        // Follow toggle (only for cards that came from the server)
                        {
                            let id_for_follow_inner = id_for_follow.clone();
                            move || {
                                let id_follow = id_for_follow_inner.clone();
                                contract_server_path.get().map(|path| {
                                    let fv = follow_view.get();
                                    let (class, title) = match &fv {
                                        Some(fv) => ("btn btn-sm btn-info", fv.tooltip.clone()),
                                        None => ("btn btn-sm btn-outline-info", format!("Follow {} on the server", path)),
                                    };
                                    view! {
                                        <button
                                            type="button"
                                            class=class
                                            on:click=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                on_toggle_follow.run(id_follow.clone());
                                            }
                                            aria-label=if fv.is_some() { "Stop following" } else { "Follow server file" }
                                            aria-pressed=if fv.is_some() { "true" } else { "false" }
                                            title=title
                                        >
                                            {broadcast_icon()}
                                        </button>
                                    }
                                })
                            }
                        }

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
                    let pending = follow_view.with(|f| f.as_ref().and_then(|f| f.pending_err.clone()));
                    if let Some(pending) = pending {
                        view! {
                            <div class="alert alert-danger py-1 px-2 mb-0" style="margin-left: 32px; font-size: 0.85em;">
                                <div><strong>"Server copy is invalid; showing the last good version."</strong></div>
                                {pending}
                            </div>
                        }.into_any()
                    } else if !error.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_stops_at_a_syntax_error() {
        assert_eq!(
            extract_agents_and_superagents("kind: Agent\nname: [x\n"),
            (vec![], vec![], vec![])
        );
        assert_eq!(
            extract_agents_and_superagents("kind: Agent\nname: a\n---\nkind: Agent\nname: [x\n"),
            (vec!["a".to_string()], vec![], vec![])
        );
    }
}
