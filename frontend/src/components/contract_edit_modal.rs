use gloo_net::http::Request;
use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{
    Event, HtmlElement, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent,
};

use super::contract_card::DiffStatus;
use crate::models::Contract;
use crate::utils::{
    check_filename_diff, compare_contracts, compute_side_by_side_diff,
    generate_unique_random_filename, validate_contract_content, validate_filename, DiffLineType,
    SideBySideDiff,
};

/// Get the API base URL (matches the pattern from JS)
fn get_api_base_url() -> String {
    // In development via Trunk, we proxy through the dev server
    // In production, we're served from the same origin as the API
    String::new() // Empty string means same origin
}

/// Fetch contract content from the server API
async fn fetch_server_contract(contract_path: &str) -> Result<Option<String>, String> {
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
async fn push_contract_to_server(contract_path: &str, content: &str) -> Result<(), String> {
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

/// Modal for editing contracts with diff view and push capability.
#[component]
pub fn ContractEditModal(
    show: ReadSignal<bool>,
    contract: ReadSignal<Option<Contract>>,
    #[prop(into)] on_hide: Callback<()>,
    #[prop(into)] on_save: Callback<Contract>,
    #[prop(into)] on_push: Callback<Contract>,
    diff_status: DiffStatus,
) -> impl IntoView {
    let _ = diff_status; // Initial diff status passed from parent (we compute our own)

    // Local state for the edited contract
    let (edited_filename, set_edited_filename) = signal(String::new());
    let (edited_content, set_edited_content) = signal(String::new());
    let (contract_id, set_contract_id) = signal(String::new());

    // Ref for focusing the modal
    let modal_ref = NodeRef::<leptos::html::Div>::new();

    // Validation errors
    let (validation_error, set_validation_error) = signal::<Option<String>>(None);
    let (filename_error, set_filename_error) = signal::<Option<String>>(None);
    let (push_error, set_push_error) = signal::<Option<String>>(None);

    // Diff state
    let (local_diff_status, set_local_diff_status) = signal(DiffStatus::default());
    let (server_text, set_server_text) = signal::<Option<String>>(None);
    let (show_diff, set_show_diff) = signal(false);
    let (show_error, set_show_error) = signal(false);

    // Push state
    let (is_pushing, set_is_pushing) = signal(false);

    // Track the original server path for the contract
    let (original_server_path, set_original_server_path) = signal::<Option<String>>(None);

    // Debounce timeout reference
    let debounce_timeout: Rc<RefCell<Option<Timeout>>> = Rc::new(RefCell::new(None));

    // Initialize edited contract when modal opens or contract changes
    Effect::new({
        let debounce_timeout = debounce_timeout.clone();
        move |_| {
            if show.get() {
                if let Some(c) = contract.get() {
                    set_contract_id.set(c.id.clone());
                    set_edited_filename.set(c.filename.clone());
                    set_edited_content.set(c.content.clone());
                    set_original_server_path.set(c.server_path.clone());

                    // Reset errors
                    set_validation_error.set(None);
                    set_filename_error.set(None);
                    set_push_error.set(None);

                    // Reset diff state
                    set_local_diff_status.set(DiffStatus::default());
                    set_server_text.set(None);
                    set_show_diff.set(false);
                    set_show_error.set(false);
                    set_is_pushing.set(false);

                    // Cancel any pending debounce
                    if let Some(timeout) = debounce_timeout.borrow_mut().take() {
                        drop(timeout);
                    }

                    // Focus the modal for keyboard events
                    if let Some(el) = modal_ref.get() {
                        let _ = el.unchecked_ref::<HtmlElement>().focus();
                    }

                    // Fetch server version for diff
                    let server_path = c.server_path.clone();
                    let filename = c.filename.clone();
                    let content = c.content.clone();

                    spawn_local(async move {
                        let path_to_check = server_path.as_deref().unwrap_or(&filename);
                        if path_to_check.trim().is_empty() {
                            return;
                        }

                        set_local_diff_status.set(DiffStatus {
                            is_different: false,
                            is_loading: true,
                            error: None,
                        });

                        match fetch_server_contract(path_to_check).await {
                            Ok(Some(fetched_text)) => {
                                set_server_text.set(Some(fetched_text.clone()));
                                let content_differs = compare_contracts(&content, &fetched_text);
                                let filename_differs =
                                    check_filename_diff(&filename, server_path.as_deref());
                                let is_different = content_differs || filename_differs;
                                set_local_diff_status.set(DiffStatus {
                                    is_different,
                                    is_loading: false,
                                    error: None,
                                });
                            }
                            Ok(None) => {
                                // Contract not found on server
                                set_server_text.set(None);
                                set_local_diff_status.set(DiffStatus {
                                    is_different: true, // Local exists but not on server
                                    is_loading: false,
                                    error: None,
                                });
                            }
                            Err(e) => {
                                set_local_diff_status.set(DiffStatus {
                                    is_different: false,
                                    is_loading: false,
                                    error: Some(e),
                                });
                            }
                        }
                    });
                }
            }
        }
    });

    // Debounced diff checking when content or filename changes
    let check_diff_debounced = {
        let debounce_timeout = debounce_timeout.clone();
        move || {
            // Cancel previous timeout
            if let Some(timeout) = debounce_timeout.borrow_mut().take() {
                drop(timeout);
            }

            let filename = edited_filename.get_untracked();
            let content = edited_content.get_untracked();
            let server_path = original_server_path.get_untracked();

            if filename.trim().is_empty() {
                return;
            }

            let timeout = Timeout::new(500, move || {
                spawn_local(async move {
                    let path_to_check = server_path.as_deref().unwrap_or(&filename);

                    match fetch_server_contract(path_to_check).await {
                        Ok(Some(fetched_text)) => {
                            set_server_text.set(Some(fetched_text.clone()));
                            let content_differs = compare_contracts(&content, &fetched_text);
                            let filename_differs =
                                check_filename_diff(&filename, server_path.as_deref());
                            let is_different = content_differs || filename_differs;
                            set_local_diff_status.update(|s| {
                                s.is_different = is_different;
                            });
                        }
                        Ok(None) => {
                            set_server_text.set(None);
                            set_local_diff_status.update(|s| {
                                s.is_different = true;
                            });
                        }
                        Err(e) => {
                            set_local_diff_status.update(|s| {
                                s.error = Some(e);
                            });
                        }
                    }
                });
            });

            *debounce_timeout.borrow_mut() = Some(timeout);
        }
    };

    // Handle filename change
    let handle_filename_change = {
        let check_diff = check_diff_debounced.clone();
        move |ev: Event| {
            let target = ev.target().unwrap();
            let input: HtmlInputElement = target.unchecked_into();
            let value = input.value();
            set_edited_filename.set(value.clone());

            // Validate filename
            if let Some(error) = validate_filename(&value) {
                set_filename_error.set(Some(error));
            } else {
                set_filename_error.set(None);
            }

            check_diff();
        }
    };

    // Handle content change
    let handle_content_change = {
        let check_diff = check_diff_debounced.clone();
        move |ev: Event| {
            let target = ev.target().unwrap();
            let textarea: HtmlTextAreaElement = target.unchecked_into();
            let value = textarea.value();
            set_edited_content.set(value.clone());

            // Validate content
            let err = validate_contract_content(&value);
            if err.is_empty() {
                set_validation_error.set(None);
            } else {
                set_validation_error.set(Some(err));
            }

            check_diff();
        }
    };

    // Handle save (local only)
    let handle_save = move |_| {
        let id = contract_id.get();
        let mut filename = edited_filename.get();
        let content = edited_content.get();

        // Generate unique filename if empty
        if filename.trim().is_empty() {
            if let Some(c) = contract.get() {
                // We need to pass the full contract list, but we don't have it here
                // Use a simple random filename instead
                filename = generate_unique_random_filename(&[c], 100);
                set_edited_filename.set(filename.clone());
            }
        }

        // Validate filename
        if let Some(error) = validate_filename(&filename) {
            set_filename_error.set(Some(error));
            return;
        }

        // Validate content
        let err = validate_contract_content(&content);
        if !err.is_empty() {
            set_validation_error.set(Some(err));
            return;
        }

        // Create the edited contract and save
        let edited = Contract {
            id,
            filename,
            content,
            err: String::new(),
            sims: HashSet::new(),
            server_path: original_server_path.get(),
        };

        on_save.run(edited);
        on_hide.run(());
    };

    // Handle push to server
    let handle_push = move |_| {
        let id = contract_id.get();
        let mut filename = edited_filename.get();
        let content = edited_content.get();
        let server_path = original_server_path.get();

        // Generate unique filename if empty
        if filename.trim().is_empty() {
            if let Some(c) = contract.get() {
                filename = generate_unique_random_filename(&[c], 100);
                set_edited_filename.set(filename.clone());
            }
        }

        // Validate filename
        if let Some(error) = validate_filename(&filename) {
            set_filename_error.set(Some(error));
            return;
        }

        // Validate content
        let err = validate_contract_content(&content);
        if !err.is_empty() {
            set_validation_error.set(Some(err));
            return;
        }

        // Save locally first
        let edited = Contract {
            id: id.clone(),
            filename: filename.clone(),
            content: content.clone(),
            err: String::new(),
            sims: HashSet::new(),
            server_path: server_path.clone(),
        };
        on_save.run(edited);

        set_is_pushing.set(true);
        set_push_error.set(None);

        spawn_local(async move {
            match push_contract_to_server(&filename, &content).await {
                Ok(()) => {
                    // Determine if filename changed
                    let filename_changed = server_path
                        .as_ref()
                        .map(|sp| sp.trim() != filename.trim())
                        .unwrap_or(false);

                    // Update serverPath: use new filename if changed
                    let new_server_path = if filename_changed {
                        filename.clone()
                    } else {
                        server_path.unwrap_or_else(|| filename.clone())
                    };

                    // Call onPush callback with updated contract data
                    let pushed = Contract {
                        id,
                        filename,
                        content,
                        err: String::new(),
                        sims: HashSet::new(), // Will be updated by parent
                        server_path: Some(new_server_path),
                    };
                    on_push.run(pushed);

                    // Update local state to reflect sync
                    set_local_diff_status.set(DiffStatus {
                        is_different: false,
                        is_loading: false,
                        error: None,
                    });
                    set_push_error.set(None);
                }
                Err(e) => {
                    set_push_error.set(Some(e));
                }
            }
            set_is_pushing.set(false);
        });
    };

    // Handle close/cancel
    let handle_close = move |_| {
        on_hide.run(());
    };

    // Keyboard shortcuts are handled via the modal's keydown event handler directly in the view

    // Collapse diff when there's no difference
    Effect::new(move |_| {
        if !local_diff_status.get().is_different {
            set_show_diff.set(false);
        }
    });

    // Compute diff for display
    let diff_result = move || -> Option<SideBySideDiff> {
        let status = local_diff_status.get();
        if !status.is_different || status.is_loading {
            return None;
        }
        let content = edited_content.get();
        let server = server_text.get();
        Some(compute_side_by_side_diff(&content, server.as_deref()))
    };

    // Determine if we're visible
    let is_visible = move || show.get();
    let current_diff_status = move || local_diff_status.get();

    // Handle keydown for keyboard shortcuts
    let handle_keydown = {
        let handle_save = handle_save.clone();
        let on_hide = on_hide.clone();
        move |ev: KeyboardEvent| {
            // Escape to close modal
            if ev.key() == "Escape" {
                ev.prevent_default();
                on_hide.run(());
            }
            // Shift+Enter to save from anywhere
            if ev.key() == "Enter" && ev.shift_key() {
                ev.prevent_default();
                handle_save(ev.unchecked_into::<MouseEvent>());
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
                    // Modal header with diff indicator
                    <div
                        class="modal-header"
                        style=move || {
                            if validation_error.get().is_some() || filename_error.get().is_some() {
                                "border-top: 3px solid #dc3545; border-left: 3px solid #dc3545; border-right: 3px solid #dc3545; border-bottom: 2px solid #dc3545; background-color: rgba(220, 53, 69, 0.1);"
                            } else if current_diff_status().is_different {
                                "border-top: 3px solid #ffc107; border-left: 3px solid #ffc107; border-right: 3px solid #ffc107; border-bottom: 2px solid #ffc107; background-color: rgba(255, 193, 7, 0.1);"
                            } else {
                                ""
                            }
                        }
                    >
                        <h5 class="modal-title" style="display: flex; align-items: center; gap: 0.5rem;">
                            "Edit Contract"
                            // Loading spinner or status badges
                            {move || {
                                if current_diff_status().is_loading {
                                    view! {
                                        <div class="spinner-border spinner-border-sm text-secondary" role="status">
                                            <span class="visually-hidden">"Loading..."</span>
                                        </div>
                                    }.into_any()
                                } else if validation_error.get().is_some() || filename_error.get().is_some() {
                                    view! {
                                        <span class="badge bg-danger d-flex align-items-center gap-1">
                                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" fill="currentColor" viewBox="0 0 16 16">
                                                <path d="M8.982 1.566a1.13 1.13 0 0 0-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 5.995A.905.905 0 0 1 8 5zm.002 6a1 1 0 1 1 0 2 1 1 0 0 1 0-2z"/>
                                            </svg>
                                            "Invalid"
                                        </span>
                                    }.into_any()
                                } else if current_diff_status().is_different {
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
                        </h5>
                        <button
                            type="button"
                            class="btn-close"
                            on:click=handle_close
                        ></button>
                    </div>

                    // Modal body
                    <div
                        class="modal-body"
                        style=move || {
                            let base = "max-height: 70vh; overflow-y: auto;";
                            if validation_error.get().is_some() || filename_error.get().is_some() {
                                format!("{} border-left: 3px solid #dc3545; border-right: 3px solid #dc3545; background-color: rgba(220, 53, 69, 0.05);", base)
                            } else if current_diff_status().is_different {
                                format!("{} border-left: 3px solid #ffc107; border-right: 3px solid #ffc107; background-color: rgba(255, 193, 7, 0.05);", base)
                            } else {
                                base.to_string()
                            }
                        }
                    >
                        // Editable filename as title
                        <div style="margin-bottom: 1rem;">
                            <input
                                type="text"
                                class=move || if filename_error.get().is_some() { "form-control is-invalid" } else { "form-control" }
                                style="font-size: 1.25rem; font-weight: 500; border: 1px solid transparent; padding: 0.25rem 0.5rem;"
                                placeholder="untitled-contract.yaml"
                                prop:value=move || edited_filename.get()
                                on:input=handle_filename_change
                            />
                            {move || filename_error.get().map(|e| view! { <div class="invalid-feedback" style="display: block;">{e}</div> })}
                        </div>

                        // Diff status - always visible, collapsible
                        <div style="margin-bottom: 1rem;">
                            <button
                                type="button"
                                class=move || {
                                    if current_diff_status().is_loading {
                                        "btn btn-outline-secondary w-100"
                                    } else if current_diff_status().is_different {
                                        "btn btn-outline-warning w-100"
                                    } else {
                                        "btn btn-outline-success w-100"
                                    }
                                }
                                style="display: flex; align-items: center; justify-content: space-between; padding: 0.5rem 1rem;"
                                disabled=move || !current_diff_status().is_different
                                on:click=move |_| set_show_diff.update(|v| *v = !*v)
                            >
                                <span style="display: flex; align-items: center; gap: 0.5rem;">
                                    {move || if current_diff_status().is_different && show_diff.get() {
                                        view! {
                                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                                <path fill-rule="evenodd" d="M1.646 4.646a.5.5 0 0 1 .708 0L8 10.293l5.646-5.647a.5.5 0 0 1 .708.708l-6 6a.5.5 0 0 1-.708 0l-6-6a.5.5 0 0 1 0-.708z"/>
                                            </svg>
                                        }.into_any()
                                    } else if current_diff_status().is_different {
                                        view! {
                                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                                <path fill-rule="evenodd" d="M4.646 1.646a.5.5 0 0 1 .708 0l6 6a.5.5 0 0 1 0 .708l-6 6a.5.5 0 0 1-.708-.708L10.293 8 4.646 2.354a.5.5 0 0 1 0-.708z"/>
                                            </svg>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                                                <path d="M12.736 3.97a.733.733 0 0 1 1.047 0c.286.289.29.756.01 1.05L7.88 12.01a.733.733 0 0 1-1.065.02L3.217 8.384a.757.757 0 0 1 0-1.06.733.733 0 0 1 1.047 0l3.052 3.093 5.4-6.425a.247.247 0 0 1 .02-.022Z"/>
                                            </svg>
                                        }.into_any()
                                    }}
                                    {move || {
                                        if current_diff_status().is_loading {
                                            "Checking server version..."
                                        } else if current_diff_status().is_different {
                                            "Differs from server version"
                                        } else {
                                            "In sync with server"
                                        }
                                    }}
                                </span>
                                {move || {
                                    if current_diff_status().is_loading {
                                        view! {
                                            <div class="spinner-border spinner-border-sm" role="status">
                                                <span class="visually-hidden">"Loading..."</span>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }
                                }}
                            </button>

                            // Collapsible diff view
                            <Show when=move || show_diff.get() && current_diff_status().is_different>
                                <div style="margin-top: 1rem;">
                                    {move || {
                                        if let Some(diff) = diff_result() {
                                            view! {
                                                <DiffView diff=diff />
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }
                                    }}
                                </div>
                            </Show>
                        </div>

                        // Push error alert
                        {move || {
                            if let Some(error) = push_error.get() {
                                view! {
                                    <div class="alert alert-danger" style="margin-bottom: 1rem;">
                                        {error}
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}

                        // Form fields
                        <div>
                            // Validation status - always visible, collapsible
                            <div style="margin-bottom: 1rem;">
                                <button
                                    type="button"
                                    class=move || {
                                        if validation_error.get().is_some() {
                                            "btn btn-outline-danger w-100"
                                        } else {
                                            "btn btn-outline-success w-100"
                                        }
                                    }
                                    style="display: flex; align-items: center; justify-content: space-between; padding: 0.5rem 1rem; text-align: left;"
                                    disabled=move || validation_error.get().is_none()
                                    on:click=move |_| set_show_error.update(|v| *v = !*v)
                                >
                                    <span style="display: flex; align-items: center; gap: 0.5rem; min-width: 0; flex: 1;">
                                        {move || if validation_error.get().is_some() && show_error.get() {
                                            view! {
                                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16" style="flex-shrink: 0;">
                                                    <path fill-rule="evenodd" d="M1.646 4.646a.5.5 0 0 1 .708 0L8 10.293l5.646-5.647a.5.5 0 0 1 .708.708l-6 6a.5.5 0 0 1-.708 0l-6-6a.5.5 0 0 1 0-.708z"/>
                                                </svg>
                                            }.into_any()
                                        } else if validation_error.get().is_some() {
                                            view! {
                                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16" style="flex-shrink: 0;">
                                                    <path fill-rule="evenodd" d="M4.646 1.646a.5.5 0 0 1 .708 0l6 6a.5.5 0 0 1 0 .708l-6 6a.5.5 0 0 1-.708-.708L10.293 8 4.646 2.354a.5.5 0 0 1 0-.708z"/>
                                                </svg>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16" style="flex-shrink: 0;">
                                                    <path d="M12.736 3.97a.733.733 0 0 1 1.047 0c.286.289.29.756.01 1.05L7.88 12.01a.733.733 0 0 1-1.065.02L3.217 8.384a.757.757 0 0 1 0-1.06.733.733 0 0 1 1.047 0l3.052 3.093 5.4-6.425a.247.247 0 0 1 .02-.022Z"/>
                                                </svg>
                                            }.into_any()
                                        }}
                                        <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                            {move || {
                                                if let Some(error) = validation_error.get() {
                                                    // Show first line or truncated error
                                                    let first_line = error.lines().next().unwrap_or(&error);
                                                    if first_line.len() > 60 {
                                                        format!("{}...", &first_line[..57])
                                                    } else {
                                                        first_line.to_string()
                                                    }
                                                } else {
                                                    "Valid YAML".to_string()
                                                }
                                            }}
                                        </span>
                                    </span>
                                </button>

                                // Collapsible full error view
                                <Show when=move || show_error.get() && validation_error.get().is_some()>
                                    <div class="alert alert-danger" style="margin-top: 0.5rem; margin-bottom: 0; white-space: pre-wrap; font-family: monospace; font-size: 0.9em;">
                                        {move || validation_error.get().unwrap_or_default()}
                                    </div>
                                </Show>
                            </div>

                            // Content textarea
                            <div class="mb-3">
                                <label class="form-label">"Contract YAML"</label>
                                <textarea
                                    class="form-control"
                                    rows="15"
                                    placeholder="Enter contract YAML"
                                    style="font-family: monospace;"
                                    prop:value=move || edited_content.get()
                                    on:input=handle_content_change
                                ></textarea>
                            </div>
                        </div>
                    </div>

                    // Modal footer
                    <div
                        class="modal-footer"
                        style=move || {
                            let base = "display: flex; justify-content: space-between; align-items: center;";
                            if validation_error.get().is_some() || filename_error.get().is_some() {
                                format!("{} border-bottom: 3px solid #dc3545; border-left: 3px solid #dc3545; border-right: 3px solid #dc3545; background-color: rgba(220, 53, 69, 0.05);", base)
                            } else if current_diff_status().is_different {
                                format!("{} border-bottom: 3px solid #ffc107; border-left: 3px solid #ffc107; border-right: 3px solid #ffc107; background-color: rgba(255, 193, 7, 0.05);", base)
                            } else {
                                base.to_string()
                            }
                        }
                    >
                        // Push button (left side)
                        <button
                            type="button"
                            class="btn btn-primary"
                            disabled=move || {
                                is_pushing.get()
                                    || validation_error.get().is_some()
                                    || filename_error.get().is_some()
                                    || !current_diff_status().is_different
                            }
                            on:click=handle_push
                        >
                            {move || {
                                if is_pushing.get() {
                                    view! {
                                        <span>
                                            <span class="spinner-border spinner-border-sm me-1" role="status"></span>
                                            "Pushing..."
                                        </span>
                                    }.into_any()
                                } else {
                                    view! { <span>"Push"</span> }.into_any()
                                }
                            }}
                        </button>

                        // Cancel and Close buttons (right side)
                        <div style="display: flex; gap: 0.5rem;">
                            <button
                                type="button"
                                class="btn btn-secondary"
                                on:click=handle_close.clone()
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class="btn btn-primary"
                                disabled=move || validation_error.get().is_some() || filename_error.get().is_some()
                                on:click=handle_save
                            >
                                "Close"
                            </button>
                        </div>
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

/// Component to render the side-by-side diff view
#[component]
fn DiffView(diff: SideBySideDiff) -> impl IntoView {
    view! {
        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 1px; border: 1px solid #dee2e6; border-radius: 4px; overflow: hidden; font-family: monospace; font-size: 0.9em; max-height: 400px; overflow-y: auto;">
            // Left column - New code (local)
            <div style="background-color: #fff;">
                <div style="padding: 0.5rem; background-color: #f6f8fa; border-bottom: 1px solid #dee2e6; font-weight: bold; font-size: 0.85em;">
                    "New (Local)"
                </div>
                {diff.left.iter().map(|line| {
                    let bg_color = match line.line_type {
                        DiffLineType::Added => "#d4edda",
                        DiffLineType::Unchanged => "#f6f8fa",
                        DiffLineType::Empty | DiffLineType::Removed => "#fff",
                    };
                    view! {
                        <div
                            style=format!("padding: 0.25rem 0.5rem; background-color: {}; white-space: pre-wrap; min-height: 1.5em;", bg_color)
                        >
                            {line.text.clone()}
                        </div>
                    }
                }).collect_view()}
            </div>

            // Right column - Old code (server)
            <div style="background-color: #fff;">
                <div style="padding: 0.5rem; background-color: #f6f8fa; border-bottom: 1px solid #dee2e6; font-weight: bold; font-size: 0.85em;">
                    "Old (Server)"
                </div>
                {diff.right.iter().map(|line| {
                    let bg_color = match line.line_type {
                        DiffLineType::Removed => "#f8d7da",
                        DiffLineType::Unchanged => "#f6f8fa",
                        DiffLineType::Empty | DiffLineType::Added => "#fff",
                    };
                    view! {
                        <div
                            style=format!("padding: 0.25rem 0.5rem; background-color: {}; white-space: pre-wrap; min-height: 1.5em;", bg_color)
                        >
                            {line.text.clone()}
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
