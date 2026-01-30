use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use promise_tracker::components::Item;
use promise_tracker::Tracker;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::models::Contract;

// Import real components
use super::contract_graph::ContractGraph;
use super::contract_text::ContractText;
use super::promise_network_graph::PromiseNetworkGraph;
use super::simulation_controls::SimulationControls;

/// Build a Tracker from contract contents
fn build_tracker_from_contracts(contracts: &[Contract], filter_sims: Option<&str>) -> Tracker {
    let mut tracker = Tracker::new();

    for contract in contracts {
        // Skip contracts with errors
        if !contract.err.is_empty() {
            continue;
        }

        // If filtering by simulation, skip contracts not in that simulation
        if let Some(sim) = filter_sims {
            if !contract.sims.contains(sim) {
                continue;
            }
        }

        // Skip empty content
        if contract.content.trim().is_empty() {
            continue;
        }

        // Parse and add items from the contract
        for document in serde_yaml::Deserializer::from_str(&contract.content) {
            if let Ok(item) = Item::deserialize(document) {
                tracker.add_item(item);
            }
        }
    }

    tracker
}

/// Main visualization container with tabs for overview and detailed view.
#[component]
pub fn ContractGrapher(
    contracts: ReadSignal<Vec<Contract>>,
    simulations: ReadSignal<Vec<String>>,
    on_add_simulation: Callback<()>,
    on_remove_simulation: Callback<()>,
) -> impl IntoView {
    // Active tab state
    let (active_view, set_active_view) = signal("overview".to_string());

    // Zoomed simulation state (None = show all, Some(sim) = show only that sim)
    let (zoomed_sim, set_zoomed_sim) = signal::<Option<String>>(None);

    // Selected component and behavior for detailed view
    let (d_component, set_d_component) = signal("---".to_string());
    let (d_behavior, set_d_behavior) = signal("---".to_string());

    // Tracker instances - one main and one per simulation
    let (main_tracker, set_main_tracker) = signal::<Option<Tracker>>(None);
    let (sim_trackers, set_sim_trackers) = signal::<HashMap<String, Tracker>>(HashMap::new());

    // Store the debounce timeout handle
    let debounce_handle: Rc<RefCell<Option<Timeout>>> = Rc::new(RefCell::new(None));

    // Debounced effect to rebuild trackers when contracts or simulations change
    let debounce_handle_clone = debounce_handle.clone();
    Effect::new(move |_| {
        let current_contracts = contracts.get();
        let sims = simulations.get();

        // Cancel any existing timeout
        if let Some(handle) = debounce_handle_clone.borrow_mut().take() {
            drop(handle);
        }

        // Create new debounced timeout
        let handle = Timeout::new(500, move || {
            // If no contracts, clear everything
            if current_contracts.is_empty() {
                set_main_tracker.set(None);
                set_sim_trackers.set(HashMap::new());
                set_d_component.set("---".to_string());
                set_d_behavior.set("---".to_string());
                return;
            }

            // Skip if any contract has an error
            if current_contracts.iter().any(|c| !c.err.is_empty()) {
                return;
            }

            // Build main tracker (no simulation filter)
            let main = build_tracker_from_contracts(&current_contracts, None);
            set_main_tracker.set(Some(main));

            // Build per-simulation trackers
            let mut sim_map = HashMap::new();
            for sim in &sims {
                let tracker = build_tracker_from_contracts(&current_contracts, Some(sim));
                sim_map.insert(sim.clone(), tracker);
            }
            set_sim_trackers.set(sim_map);
        });

        *debounce_handle_clone.borrow_mut() = Some(handle);
    });

    // Get component names from the main tracker
    let components = move || {
        main_tracker
            .get()
            .map(|t| {
                t.get_working_agent_names()
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    // Get wants (behaviors) for the selected component
    let wants = move || {
        let component = d_component.get();
        if component == "---" {
            return vec![("---".to_string(), "Select Component First".to_string())];
        }

        main_tracker
            .get()
            .and_then(|t| {
                if t.has_agent(component.clone()) {
                    let agent_wants: Vec<String> =
                        t.get_agent_wants(component).into_iter().collect();
                    if agent_wants.is_empty() {
                        Some(vec![(
                            "---".to_string(),
                            "This component has no wants entries".to_string(),
                        )])
                    } else {
                        let mut options =
                            vec![("---".to_string(), "Select a behavior".to_string())];
                        let mut sorted_wants = agent_wants;
                        sorted_wants.sort();
                        options.extend(sorted_wants.into_iter().map(|w| (w.clone(), w)));
                        Some(options)
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| vec![("---".to_string(), "Select Component First".to_string())])
    };

    // Check if behavior dropdown should be enabled
    let wants_valid = move || {
        let component = d_component.get();
        if component == "---" {
            return false;
        }

        main_tracker
            .get()
            .map(|t| t.has_agent(component.clone()) && !t.get_agent_wants(component).is_empty())
            .unwrap_or(false)
    };

    // Handle component dropdown change
    let on_component_change = move |ev: web_sys::Event| {
        let target = event_target::<web_sys::HtmlSelectElement>(&ev);
        set_d_component.set(target.value());
        set_d_behavior.set("---".to_string());
    };

    // Handle behavior dropdown change
    let on_behavior_change = move |ev: web_sys::Event| {
        let target = event_target::<web_sys::HtmlSelectElement>(&ev);
        set_d_behavior.set(target.value());
    };

    // Create signal wrappers for sim trackers to pass to child components
    let get_sim_tracker = move |sim: &str| -> Signal<Option<Tracker>> {
        let sim_owned = sim.to_string();
        Signal::derive(move || sim_trackers.get().get(&sim_owned).cloned())
    };

    // Compute visible simulations based on zoom state
    let visible_simulations = move || -> Vec<String> {
        match zoomed_sim.get() {
            Some(sim) => vec![sim],
            None => simulations.get(),
        }
    };

    // Effect to clear zoom if the zoomed simulation is removed
    Effect::new(move |_| {
        let current_sims = simulations.get();
        if let Some(sim) = zoomed_sim.get() {
            if !current_sims.contains(&sim) {
                set_zoomed_sim.set(None);
            }
        }
    });

    // Zoom callback for SimulationControls
    let on_zoom = Callback::new(move |sim: Option<String>| {
        set_zoomed_sim.set(sim);
    });

    view! {
        <div class="contract-grapher">
            <h1 class="header">"Contract"</h1>

            // Simulation controls
            <SimulationControls
                simulations=simulations
                zoomed_sim=zoomed_sim
                on_zoom=on_zoom
                on_add=on_add_simulation
                on_remove=on_remove_simulation
            />

            // Bootstrap Nav Tabs
            <ul class="nav nav-tabs mb-3">
                <li class="nav-item">
                    <a
                        class="nav-link"
                        class:active=move || active_view.get() == "overview"
                        href="#"
                        on:click=move |ev| {
                            ev.prevent_default();
                            set_active_view.set("overview".to_string());
                        }
                    >
                        "Overview"
                    </a>
                </li>
                <li class="nav-item">
                    <a
                        class="nav-link"
                        class:active=move || active_view.get() == "detailed"
                        href="#"
                        on:click=move |ev| {
                            ev.prevent_default();
                            set_active_view.set("detailed".to_string());
                        }
                    >
                        "Detailed View"
                    </a>
                </li>
            </ul>

            // Tab Content
            <div class="tab-content">
                // Overview Tab
                <Show when=move || active_view.get() == "overview">
                    <div style="margin-top: 1rem;">
                        <div style="margin-bottom: 1rem;">
                            <h3 style="margin: 0;">"Promise Relationships Overview"</h3>
                            <p style="color: #666; font-size: 0.9em; margin-top: 0.25rem; margin-bottom: 0;">
                                "This view shows all components and their promise relationships."
                            </p>
                        </div>
                        <div style="display: flex; gap: 1rem; margin-top: 1rem;">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let sim_id = format!("network-{}", sim);
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    view! {
                                        <div
                                            style="flex: 1; min-width: 0; border: 1px solid #ccc; border-radius: 4px; padding: 0.5rem; background: #fafbfc;"
                                        >
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>
                                            <PromiseNetworkGraph tracker=tracker_signal sim_id=sim_id />
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                            }}
                        </div>
                    </div>
                </Show>

                // Detailed View Tab
                <Show when=move || active_view.get() == "detailed">
                    <div style="margin-top: 1rem;">
                        <h3>"Detailed Promise Resolution"</h3>
                        <p style="color: #666; font-size: 0.9em; margin-bottom: 1rem;">
                            "Select a component and behavior to see how promises are resolved."
                        </p>

                        // Component and Behavior dropdowns
                        <div class="mb-3">
                            <select
                                class="form-select mb-2"
                                on:change=on_component_change
                            >
                                {move || {
                                    let current = d_component.get();
                                    let is_default = current == "---";
                                    let mut opts = vec![view! {
                                        <option value="---" selected=is_default>
                                            "Select a Component"
                                        </option>
                                    }
                                    .into_any()];
                                    opts.extend(components().into_iter().map(|c| {
                                        let c_value = c.clone();
                                        let c_display = c.clone();
                                        let is_selected = c == current;
                                        view! {
                                            <option value=c_value selected=is_selected>
                                                {c_display}
                                            </option>
                                        }
                                        .into_any()
                                    }));
                                    opts
                                }}
                            </select>
                            <select
                                class="form-select"
                                on:change=on_behavior_change
                                disabled=move || !wants_valid()
                            >
                                {move || {
                                    let current = d_behavior.get();
                                    wants()
                                        .into_iter()
                                        .map(|(value, display)| {
                                            let is_selected = value == current;
                                            view! {
                                                <option value=value selected=is_selected>
                                                    {display}
                                                </option>
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                }}
                            </select>
                        </div>

                        // Simulation panels
                        <div style="display: flex; gap: 1rem; margin-top: 1rem;">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    // Local tab state for each simulation's text/sequence tabs
                                    let (inner_tab, set_inner_tab) = signal("text".to_string());
                                    view! {
                                        <div
                                            style="flex: 1; min-width: 0; border: 1px solid #ccc; border-radius: 4px; padding: 0.5rem; background: #fafbfc;"
                                        >
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>

                                            // Inner tabs for Text View / Sequence Diagram
                                            <ul class="nav nav-tabs mb-2">
                                                <li class="nav-item">
                                                    <a
                                                        class="nav-link"
                                                        class:active=move || inner_tab.get() == "text"
                                                        href="#"
                                                        on:click=move |ev| {
                                                            ev.prevent_default();
                                                            set_inner_tab.set("text".to_string());
                                                        }
                                                    >
                                                        "Text View"
                                                    </a>
                                                </li>
                                                <li class="nav-item">
                                                    <a
                                                        class="nav-link"
                                                        class:active=move || inner_tab.get() == "sequence"
                                                        href="#"
                                                        on:click=move |ev| {
                                                            ev.prevent_default();
                                                            set_inner_tab.set("sequence".to_string());
                                                        }
                                                    >
                                                        "Sequence Diagram"
                                                    </a>
                                                </li>
                                            </ul>

                                            // Inner tab content
                                            <Show when=move || inner_tab.get() == "text">
                                                <ContractText
                                                    tracker=tracker_signal
                                                    selected_component=d_component
                                                    selected_behavior=d_behavior
                                                />
                                            </Show>
                                            <Show when=move || inner_tab.get() == "sequence">
                                                <ContractGraph
                                                    tracker=tracker_signal
                                                    selected_component=d_component
                                                    selected_behavior=d_behavior
                                                />
                                            </Show>
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                            }}
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}
