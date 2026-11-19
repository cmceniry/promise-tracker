use gloo_storage::{LocalStorage, Storage};
use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use promise_tracker::components::Item;
use promise_tracker::fold::{FoldOptions, GroupSpec};
use promise_tracker::Tracker;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::follow::{blocks_graph, in_graph};
use crate::models::Contract;

// Import real components
use super::contract_graph::ContractGraph;
use super::contract_text::ContractText;
use super::cycle_banner::CycleBanner;
use super::display_options::{DisplayOptions, ViewOptions};
use super::inventory_view::{InventoryState, InventoryView};
use super::promise_graph_view::PromiseGraphView;
use super::simulation_controls::SimulationControls;
use super::trace_view::TraceView;

const GROUPS_STORAGE_KEY: &str = "scratch_groups";

/// The items of a contract that can be drawn: documents that are not an
/// Item are skipped, and reading stops at the first YAML syntax error —
/// after one, serde_yaml's document iterator yields that same error forever.
fn graphable_items(content: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for document in serde_yaml::Deserializer::from_str(content) {
        let Ok(value) = serde_yaml::Value::deserialize(document) else {
            break;
        };
        if let Ok(item) = Item::deserialize(value) {
            items.push(item);
        }
    }
    items
}

/// Build a Tracker from contract contents. `fold_collectives` chooses whether
/// a SuperAgent stands as one agent or as its members.
fn build_tracker_from_contracts(
    contracts: &[Contract],
    filter_sims: Option<&str>,
    fold_collectives: bool,
) -> Tracker {
    let mut tracker = Tracker::new();
    tracker.set_fold_collectives(fold_collectives);

    let mut items = Vec::new();
    for contract in contracts {
        // Skip contracts with errors (a followed card in error has no good
        // version to show; one with a pending bad copy still has `content`)
        if !in_graph(contract) {
            continue;
        }

        // If filtering by simulation, skip contracts not in that simulation
        if let Some(sim) = filter_sims {
            if !contract.sims.contains(sim) {
                continue;
            }
        }

        items.extend(graphable_items(&contract.content));
    }
    tracker.add_items(items);

    tracker
}

/// Main visualization container with tabs for overview and detailed view.
#[component]
pub fn ContractGrapher(
    contracts: ReadSignal<Vec<Contract>>,
    simulations: ReadSignal<Vec<String>>,
    /// False until followed cards have had their first fetch of the session;
    /// the graph waits rather than draw their stored copies and then redraw.
    #[prop(into)]
    ready: Signal<bool>,
    on_add_simulation: Callback<()>,
    on_remove_simulation: Callback<()>,
) -> impl IntoView {
    // Active tab state
    let (active_view, set_active_view) = signal("overview".to_string());

    // Zoomed simulation state (None = show all, Some(sim) = show only that sim)
    let (zoomed_sim, set_zoomed_sim) = signal::<Option<String>>(None);

    // Display options, applied to every view and every simulation panel.
    // Read these only inside the child components: reading one in the per-sim
    // closures below would rebuild every panel on a toggle, resetting each
    // one's inner tab and tearing down its graph.
    let (view_options, set_view_options) = signal(ViewOptions::default());
    let view_options_signal: Signal<ViewOptions> = view_options.into();

    // The SuperAgent switch changes how the trackers are built, so it is
    // memoized apart: the other switches must not rebuild them.
    let fold_collectives = Memo::new(move |_| view_options.get().collectives);

    // Scratch groups the reader drew on the overview. They are how the reader
    // is looking, not what the contract says, so they live in this browser.
    let (groups, set_groups) =
        signal::<Vec<GroupSpec>>(LocalStorage::get(GROUPS_STORAGE_KEY).unwrap_or_default());
    Effect::new(move |_| {
        if let Err(e) = LocalStorage::set(GROUPS_STORAGE_KEY, groups.get()) {
            web_sys::console::error_1(&format!("Failed to save scratch groups: {:?}", e).into());
        }
    });
    let on_groups_change = Callback::new(move |next: Vec<GroupSpec>| set_groups.set(next));

    // The overview's folding rules, read off the display options. Scratch
    // groups are the reader's drawing, not an option, and ride along.
    let effective_fold = Signal::derive(move || {
        let o = view_options.get();
        FoldOptions {
            collectives: o.collectives,
            bundle: o.bundle,
            own_inside: o.fold_self,
            groups: groups.get(),
        }
    });

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
        let fold = fold_collectives.get();
        if !ready.get() {
            return;
        }

        // Cancel any existing timeout
        if let Some(handle) = debounce_handle_clone.borrow_mut().take() {
            drop(handle);
        }

        // A short debounce only to coalesce bursts (a followed directory
        // landing many cards at once); a rebuild itself is cheap.
        let handle = Timeout::new(50, move || {
            // If no contracts, clear everything
            if current_contracts.is_empty() {
                set_main_tracker.set(None);
                set_sim_trackers.set(HashMap::new());
                set_d_component.set("---".to_string());
                set_d_behavior.set("---".to_string());
                return;
            }

            // Skip if any (unfollowed) contract has an error; a followed card
            // in error is only left out, never holds the graph back
            if blocks_graph(&current_contracts) {
                return;
            }

            // Build main tracker (no simulation filter)
            let main = build_tracker_from_contracts(&current_contracts, None, fold);
            set_main_tracker.set(Some(main));

            // Build per-simulation trackers
            let mut sim_map = HashMap::new();
            for sim in &sims {
                let tracker = build_tracker_from_contracts(&current_contracts, Some(sim), fold);
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

    // Edge tap in the overview graph jumps to the Detailed view with the
    // component and behavior preselected. A per-sim edge may not exist in the
    // main tracker that drives the dropdowns, so validate before navigating.
    let on_edge_select = Callback::new(move |(component, behavior): (String, String)| {
        let valid = main_tracker
            .get_untracked()
            .map(|t| {
                t.has_agent(component.clone())
                    && t.get_agent_wants(component.clone()).contains(&behavior)
            })
            .unwrap_or(false);
        if valid {
            set_d_component.set(component);
            set_d_behavior.set(behavior);
            set_active_view.set("detailed".to_string());
        }
    });

    // The Trace view's want list shares the Detailed view's selection, so
    // moving between the two tabs keeps the same want in front.
    let on_trace_select = Callback::new(move |(component, behavior): (String, String)| {
        set_d_component.set(component);
        set_d_behavior.set(behavior);
    });

    // The Inventory's inner tab and selection, shared by every panel.
    let inventory = InventoryState::new();

    // The agent selection follows the SuperAgents fold: on, a member moves
    // to the SuperAgent folding it; off, it moves back. The main tracker is
    // read before the rebuild lands, which is fine: what folds what does not
    // depend on how the tracker was built.
    Effect::new(move |previous: Option<bool>| {
        let folded = fold_collectives.get();
        if previous.is_some_and(|p| p != folded) {
            main_tracker.with_untracked(|t| {
                if let Some(t) = t {
                    inventory.follow_fold(t, folded);
                }
            });
        }
        folded
    });

    // An Unsatisfied row's trace link opens the Trace tab on that want.
    let on_inventory_trace = Callback::new(move |(component, behavior): (String, String)| {
        set_d_component.set(component);
        set_d_behavior.set(behavior);
        set_active_view.set("trace".to_string());
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

            // Display option toggles, applying to every view below
            <DisplayOptions options=view_options set_options=set_view_options />

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
                <li class="nav-item">
                    <a
                        class="nav-link"
                        class:active=move || active_view.get() == "trace"
                        href="#"
                        on:click=move |ev| {
                            ev.prevent_default();
                            set_active_view.set("trace".to_string());
                        }
                    >
                        "Trace"
                    </a>
                </li>
                <li class="nav-item">
                    <a
                        class="nav-link"
                        class:active=move || active_view.get() == "inventory"
                        href="#"
                        on:click=move |ev| {
                            ev.prevent_default();
                            set_active_view.set("inventory".to_string());
                        }
                    >
                        "Inventory"
                    </a>
                </li>
            </ul>

            // Tab Content
            <div class="tab-content grapher-tabs">
                // Overview Tab
                <Show when=move || active_view.get() == "overview">
                    <div class="grapher-pane">
                        <div style="margin-bottom: 1rem;">
                            <h3 style="margin: 0;">"Promise Relationships Overview"</h3>
                            <p style="color: #666; font-size: 0.9em; margin-top: 0.25rem; margin-bottom: 0;">
                                "This view shows all components and their promise relationships."
                            </p>
                        </div>
                        <div class="grapher-sims">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let sim_id = format!("network-{}", sim);
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    let sim_for_graph = sim.clone();
                                    view! {
                                        <div class="grapher-sim">
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>
                                            <CycleBanner tracker=tracker_signal />
                                            <PromiseGraphView
                                                tracker=tracker_signal
                                                sim_id=sim_id
                                                fold_options=effective_fold
                                                groups=groups
                                                on_groups_change=on_groups_change
                                                sim_label=sim_for_graph.clone()
                                                on_edge_select=on_edge_select
                                            />
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
                    <div class="grapher-pane">
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
                        <div class="grapher-sims">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    let sim_for_text = sim.clone();
                                    let sim_for_sequence = sim.clone();
                                    // Local tab state for each simulation's text/sequence tabs
                                    let (inner_tab, set_inner_tab) = signal("text".to_string());
                                    view! {
                                        <div class="grapher-sim">
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>
                                            <CycleBanner tracker=tracker_signal />

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
                                                    options=view_options_signal
                                                    sim_label=sim_for_text.clone()
                                                />
                                            </Show>
                                            <Show when=move || inner_tab.get() == "sequence">
                                                <ContractGraph
                                                    tracker=tracker_signal
                                                    selected_component=d_component
                                                    selected_behavior=d_behavior
                                                    options=view_options_signal
                                                    sim_label=sim_for_sequence.clone()
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

                // Trace Tab
                <Show when=move || active_view.get() == "trace">
                    <div class="grapher-pane">
                        <div style="margin-bottom: 1rem;">
                            <h3 style="margin: 0;">"Trace a Want"</h3>
                            <p style="color: #666; font-size: 0.9em; margin-top: 0.25rem; margin-bottom: 0;">
                                "One want, read left to right: who promises it, and what that rests on. Each promise is shown once."
                            </p>
                        </div>
                        <div class="grapher-sims">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    let sim_for_trace = sim.clone();
                                    view! {
                                        <div class="grapher-sim">
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>
                                            <CycleBanner tracker=tracker_signal />
                                            <TraceView
                                                tracker=tracker_signal
                                                selected_component=d_component
                                                selected_behavior=d_behavior
                                                on_select=on_trace_select
                                                options=view_options_signal
                                                sim_label=sim_for_trace
                                            />
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                            }}
                        </div>
                    </div>
                </Show>

                // Inventory Tab
                <Show when=move || active_view.get() == "inventory">
                    <div class="grapher-pane">
                        <div style="margin-bottom: 1rem;">
                            <h3 style="margin: 0;">"Inventory"</h3>
                            <p style="color: #666; font-size: 0.9em; margin-top: 0.25rem; margin-bottom: 0;">
                                "What each agent provides, who provides and uses each behavior, which wants nobody keeps, and which promises nobody uses. Click a name for a summary."
                            </p>
                        </div>
                        <div class="grapher-sims">
                            {move || {
                                visible_simulations()
                                    .iter()
                                    .map(|sim| {
                                    let tracker_signal = get_sim_tracker(sim);
                                    let sim_display = sim.clone();
                                    view! {
                                        <div class="grapher-sim">
                                            <div style="font-weight: bold; margin-bottom: 0.5rem; text-align: center;">
                                                "Simulation " {sim_display}
                                            </div>
                                            <CycleBanner tracker=tracker_signal />
                                            <InventoryView
                                                tracker=tracker_signal
                                                state=inventory
                                                options=view_options_signal
                                                on_trace=on_inventory_trace
                                            />
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphable_items_stops_at_a_syntax_error() {
        assert!(graphable_items("kind: Agent\nname: [x\n").is_empty());
        let items = graphable_items("kind: Agent\nname: a\n---\nkind: Agent\nname: [x\n");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn graphable_items_skips_documents_that_are_not_items() {
        let items = graphable_items(
            "kind: Agent\nname: a\n---\nkind: Bogus\nname: b\n---\nkind: SuperAgent\nname: sa\n",
        );
        assert_eq!(
            items.iter().map(Item::get_name).collect::<Vec<String>>(),
            vec!["Agent/a", "SuperAgent/sa"]
        );
    }
}
