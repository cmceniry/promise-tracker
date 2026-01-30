use leptos::prelude::*;
use leptos_drag_reorder::{provide_drag_reorder, use_drag_reorder, HoverPosition};
use oco_ref::Oco;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileReader, HtmlElement, HtmlInputElement, KeyboardEvent};

use crate::components::{ContractBrowser, ContractCard, ContractEditModal, DiffStatus};
use crate::models::Contract;
use crate::utils::{generate_unique_random_filename, validate_contract_content, validate_filename};

/// Container managing multiple contract cards with drag-and-drop.
#[component]
pub fn ContractCarder(
    contracts: ReadSignal<Vec<Contract>>,
    set_contracts: WriteSignal<Vec<Contract>>,
    simulations: ReadSignal<Vec<String>>,
) -> impl IntoView {
    // Modal visibility signals
    let (show_upload_modal, set_show_upload_modal) = signal(false);
    let (show_browser_modal, set_show_browser_modal) = signal(false);
    let (show_edit_modal, set_show_edit_modal) = signal(false);
    let (editing_contract, set_editing_contract) = signal::<Option<Contract>>(None);

    // Ref for focusing the upload modal
    let upload_modal_ref = NodeRef::<leptos::html::Div>::new();

    // Create panel order signal for drag reorder (single column)
    // Uses Oco<'static, str> as required by leptos_drag_reorder
    let panel_order: RwSignal<Vec<Oco<'static, str>>> = RwSignal::new(
        contracts
            .get_untracked()
            .iter()
            .map(|c| Oco::from(c.id.clone()))
            .collect::<Vec<_>>(),
    );

    // Sync panel_order when contracts change (e.g., add/delete)
    Effect::new(move |_| {
        let current_contracts = contracts.get();
        let current_order = panel_order.get();

        // Get IDs from contracts
        let contract_ids: HashSet<String> =
            current_contracts.iter().map(|c| c.id.clone()).collect();
        let order_ids: HashSet<String> = current_order.iter().map(|o| o.to_string()).collect();

        // Check if we need to update (new contracts added or contracts removed)
        if contract_ids != order_ids {
            // Keep existing order for contracts that still exist, append new ones
            let mut new_order: Vec<Oco<'static, str>> = current_order
                .into_iter()
                .filter(|id| {
                    let id_str: &str = id.as_ref();
                    contract_ids.contains(id_str)
                })
                .collect();

            // Add any new contracts at the end
            for contract in &current_contracts {
                if !new_order
                    .iter()
                    .any(|id| id.as_ref() as &str == contract.id.as_str())
                {
                    new_order.push(Oco::from(contract.id.clone()));
                }
            }

            panel_order.set(new_order);
        }
    });

    // Sync contracts order when panel_order changes (from drag reorder)
    Effect::new(move |_| {
        let order = panel_order.get();
        set_contracts.update(|c| {
            // Only reorder if the order actually changed
            let current_order: Vec<String> = c.iter().map(|contract| contract.id.clone()).collect();
            let new_order: Vec<String> = order.iter().map(|o| o.to_string()).collect();
            if current_order != new_order {
                c.sort_by_key(|contract| {
                    order
                        .iter()
                        .position(|id| (id.as_ref() as &str) == contract.id.as_str())
                        .unwrap_or(usize::MAX)
                });
            }
        });
    });

    // Provide drag reorder context (single column)
    let [column_ref]: [NodeRef<leptos::html::Div>; 1] = provide_drag_reorder([panel_order]);

    // Add a blank contract
    let add_blank_contract = move |_| {
        let current = contracts.get();
        let filename = generate_unique_random_filename(&current, 100);
        let sims = simulations.get();
        let new_contract = Contract::with_default_sims(filename, String::new(), &sims);
        set_contracts.update(|c| c.push(new_contract));
    };

    // Open upload modal
    let open_upload_modal = move |_| {
        set_show_upload_modal.set(true);
    };

    // Close upload modal
    let close_upload_modal = move |_| {
        set_show_upload_modal.set(false);
    };

    // Handle escape key for upload modal
    let handle_upload_keydown = move |ev: KeyboardEvent| {
        if ev.key() == "Escape" {
            ev.prevent_default();
            set_show_upload_modal.set(false);
        }
    };

    // Focus upload modal when it opens
    Effect::new(move |_| {
        if show_upload_modal.get() {
            if let Some(el) = upload_modal_ref.get() {
                let _ = el.unchecked_ref::<HtmlElement>().focus();
            }
        }
    });

    // Open browser modal
    let open_browser_modal = move |_| {
        set_show_browser_modal.set(true);
    };

    // Close browser modal
    let close_browser_modal = Callback::new(move |_: ()| {
        set_show_browser_modal.set(false);
    });

    // Handle file selection and upload
    let handle_file_change = move |ev: Event| {
        let target = ev.target().unwrap();
        let input: HtmlInputElement = target.unchecked_into();

        if let Some(files) = input.files() {
            if let Some(file) = files.get(0) {
                let filename = file.name();

                // Validate filename
                if let Some(error) = validate_filename(&filename) {
                    web_sys::window()
                        .unwrap()
                        .alert_with_message(&format!(
                            "Invalid filename: {}\n\nFile: {}",
                            error, filename
                        ))
                        .ok();
                    return;
                }

                // Check for duplicate filename
                let current = contracts.get();
                if current.iter().any(|c| c.filename == filename) {
                    web_sys::window()
                        .unwrap()
                        .alert_with_message(&format!(
                            "A contract with filename \"{}\" already exists. Please rename the file or remove the existing contract.",
                            filename
                        ))
                        .ok();
                    return;
                }

                // Read file contents
                let reader = FileReader::new().unwrap();
                let reader_clone = reader.clone();
                let filename_clone = filename.clone();
                let sims = simulations.get();

                let onload = Closure::wrap(Box::new(move |_: Event| {
                    if let Ok(result) = reader_clone.result() {
                        if let Some(text) = result.as_string() {
                            let err = validate_contract_content(&text);
                            let mut contract =
                                Contract::with_default_sims(filename_clone.clone(), text, &sims);
                            contract.err = err;
                            set_contracts.update(|c| c.push(contract));
                            set_show_upload_modal.set(false);
                        }
                    }
                }) as Box<dyn FnMut(_)>);

                reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                reader.read_as_text(&file).ok();
                onload.forget(); // Prevent closure from being dropped
            }
        }

        // Reset the input so the same file can be selected again
        input.set_value("");
    };

    // Handle contract loaded from API browser
    let handle_api_load = Callback::new(
        move |(contract_id, filename, content): (String, String, String)| {
            let sims = simulations.get();
            let err = validate_contract_content(&content);
            let mut contract = Contract::with_default_sims(filename.clone(), content, &sims);
            contract.server_path = Some(contract_id);
            contract.err = err;
            set_contracts.update(|c| c.push(contract));
            // Modal will stay open - user can close it manually
        },
    );

    // Delete a contract
    let delete_contract = Callback::new(move |contract_id: String| {
        set_contracts.update(|c| c.retain(|contract| contract.id != contract_id));
    });

    // Toggle simulation for a contract
    let toggle_sim = Callback::new(move |(contract_id, sim): (String, String)| {
        set_contracts.update(|c| {
            if let Some(contract) = c.iter_mut().find(|contract| contract.id == contract_id) {
                contract.toggle_sim(&sim);
            }
        });
    });

    // Open edit modal for a contract
    let open_edit = Callback::new(move |contract_id: String| {
        let current = contracts.get();
        if let Some(contract) = current.iter().find(|c| c.id == contract_id) {
            set_editing_contract.set(Some(contract.clone()));
            set_show_edit_modal.set(true);
        }
    });

    // Close edit modal
    let close_edit_modal = Callback::new(move |_: ()| {
        set_show_edit_modal.set(false);
        set_editing_contract.set(None);
    });

    // Save edited contract
    let save_contract = Callback::new(move |edited: Contract| {
        set_contracts.update(|c| {
            if let Some(contract) = c.iter_mut().find(|contract| contract.id == edited.id) {
                contract.filename = edited.filename;
                contract.content = edited.content;
                contract.err = validate_contract_content(&contract.content);
            }
        });
    });

    // Push contract to server (stub for now)
    let push_contract = Callback::new(move |pushed: Contract| {
        set_contracts.update(|c| {
            if let Some(contract) = c.iter_mut().find(|contract| contract.id == pushed.id) {
                contract.filename = pushed.filename;
                contract.content = pushed.content;
                contract.server_path = pushed.server_path;
            }
        });
    });

    // Get downloaded contract paths for the browser (as a Memo for reactive tracking)
    let downloaded_paths = Memo::new(move |_| {
        contracts
            .get()
            .iter()
            .filter_map(|c| c.server_path.clone())
            .collect::<HashSet<String>>()
    });

    // Get editing contract's sims (as a Memo for reactive tracking)
    let editing_sims = Memo::new(move |_| {
        editing_contract
            .get()
            .and_then(|ec| {
                contracts
                    .get()
                    .iter()
                    .find(|c| c.id == ec.id)
                    .map(|c| c.sims.clone())
            })
            .unwrap_or_default()
    });

    view! {
        <div style="height: 100vh; overflow-y: auto;">
            // Contract cards list - iterate in panel_order to maintain drag order
            // The column_ref must be attached to the container for drag reorder to work
            <div node_ref=column_ref>
                <For
                    each=move || {
                        let order = panel_order.get();
                        let all_contracts = contracts.get();
                        order
                            .into_iter()
                            .enumerate()
                            .filter_map(|(index, id)| {
                                all_contracts
                                    .iter()
                                    .find(|c| c.id == id)
                                    .map(|c| (index, c.clone()))
                            })
                            .collect::<Vec<_>>()
                    }
                    key=|(_, c)| c.id.clone()
                    children=move |(index, contract)| {
                        let card_class = if index % 2 == 0 {
                            "contract-card-even"
                        } else {
                            "contract-card-odd"
                        };

                        // Create reactive signals for this contract's properties
                        let contract_id_for_reactive = contract.id.clone();
                        let contract_sims_signal = Memo::new(move |_| {
                            contracts
                                .get()
                                .iter()
                                .find(|c| c.id == contract_id_for_reactive)
                                .map(|c| c.sims.clone())
                                .unwrap_or_default()
                        });

                        let contract_id_for_filename = contract.id.clone();
                        let contract_filename_signal = Memo::new(move |_| {
                            contracts
                                .get()
                                .iter()
                                .find(|c| c.id == contract_id_for_filename)
                                .map(|c| c.filename.clone())
                                .unwrap_or_default()
                        });

                        let contract_id_for_content = contract.id.clone();
                        let contract_content_signal = Memo::new(move |_| {
                            contracts
                                .get()
                                .iter()
                                .find(|c| c.id == contract_id_for_content)
                                .map(|c| c.content.clone())
                                .unwrap_or_default()
                        });

                        let contract_id_for_error = contract.id.clone();
                        let contract_error_signal = Memo::new(move |_| {
                            contracts
                                .get()
                                .iter()
                                .find(|c| c.id == contract_id_for_error)
                                .map(|c| c.err.clone())
                                .unwrap_or_default()
                        });

                        view! {
                            <DraggableContractCard
                                contract_id=contract.id.clone()
                                card_class=card_class.to_string()
                                contract_filename=contract_filename_signal
                                contract_content=contract_content_signal
                                contract_error=contract_error_signal
                                contract_sims=contract_sims_signal
                                on_delete=delete_contract
                                on_toggle_sim=toggle_sim
                                on_edit=open_edit
                                simulations=simulations
                            />
                        }
                    }
                />
            </div>

            // Add blank contract button
            <div class="card mb-2">
                <button
                    class="btn btn-outline-primary w-100"
                    on:click=add_blank_contract
                    aria-label="Add Another Contract"
                >
                    <span class="me-1">"+"</span>
                    "New Contract"
                </button>
            </div>

            // Upload contract button
            <div class="card mb-2">
                <button
                    class="btn btn-outline-secondary w-100"
                    on:click=open_upload_modal
                    aria-label="Upload Contract"
                >
                    <span class="me-1">"↑"</span>
                    "Upload Contract"
                </button>
            </div>

            // Load from server button
            <div class="card mb-2">
                <button
                    class="btn btn-outline-info w-100"
                    on:click=open_browser_modal
                    aria-label="Load Contract from API"
                >
                    <span class="me-1">"☁"</span>
                    "Load from Server"
                </button>
            </div>

            // Upload modal
            <div
                class="modal"
                class:show=move || show_upload_modal.get()
                style:display=move || if show_upload_modal.get() { "block" } else { "none" }
                tabindex="-1"
                on:keydown=handle_upload_keydown
                node_ref=upload_modal_ref
            >
                <div class="modal-dialog">
                    <div class="modal-content">
                        <div class="modal-header">
                            <h5 class="modal-title">"Upload Contract File"</h5>
                            <button
                                type="button"
                                class="btn-close"
                                on:click=close_upload_modal
                            ></button>
                        </div>
                        <div class="modal-body">
                            <input
                                type="file"
                                class="form-control"
                                accept=".yaml,.yml"
                                on:change=handle_file_change
                            />
                        </div>
                        <div class="modal-footer">
                            <button
                                type="button"
                                class="btn btn-secondary"
                                on:click=close_upload_modal
                            >
                                "Cancel"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
            // Upload modal backdrop
            <Show when=move || show_upload_modal.get()>
                <div class="modal-backdrop fade show"></div>
            </Show>

            // Contract browser modal
            <ContractBrowser
                show=show_browser_modal
                on_hide=close_browser_modal
                on_select_contract=handle_api_load
                downloaded_contract_paths=downloaded_paths
            />

            // Contract edit modal
            <ContractEditModal
                show=show_edit_modal
                contract=editing_contract
                on_hide=close_edit_modal
                on_save=save_contract
                on_push=push_contract
                simulations=simulations
                contract_sims=editing_sims
                on_toggle_sim=toggle_sim
                diff_status=DiffStatus::default()
            />
        </div>
    }
}

/// Wrapper component for a draggable contract card using leptos_drag_reorder
#[component]
fn DraggableContractCard(
    contract_id: String,
    card_class: String,
    #[prop(into)] contract_filename: Signal<String>,
    #[prop(into)] contract_content: Signal<String>,
    #[prop(into)] contract_error: Signal<String>,
    #[prop(into)] contract_sims: Signal<HashSet<String>>,
    #[prop(into)] on_delete: Callback<String>,
    #[prop(into)] on_toggle_sim: Callback<(String, String)>,
    #[prop(into)] on_edit: Callback<String>,
    #[prop(into)] simulations: Signal<Vec<String>>,
) -> impl IntoView {
    let drag = use_drag_reorder(contract_id.clone());

    // Compute border style based on hover position
    let border_style = move || match drag.hover_position.get() {
        Some(HoverPosition::Above) => "border-top: 3px solid #0d6efd; margin-top: -3px;",
        Some(HoverPosition::Below) => "border-bottom: 3px solid #0d6efd; margin-bottom: -3px;",
        None => "",
    };

    // Enable dragging by default
    Effect::new(move |_| {
        (drag.set_draggable)(true);
    });

    view! {
        <div
            node_ref=drag.node_ref
            draggable=move || if drag.draggable.get() { "true" } else { "false" }
            on:dragstart=drag.on_dragstart
            on:dragend=drag.on_dragend
            style=border_style
        >
            <ContractCard
                contract_id=contract_id
                contract_filename=contract_filename
                contract_content=contract_content
                contract_error=contract_error
                contract_sims=contract_sims
                on_delete=on_delete
                on_toggle_sim=on_toggle_sim
                on_edit=on_edit
                simulations=simulations
                card_class_name=card_class
                diff_status=DiffStatus::default()
            />
        </div>
    }
}
