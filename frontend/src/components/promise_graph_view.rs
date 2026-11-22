//! PromiseGraphView component - edge-as-promise graph rendered with Cytoscape + dagre.
//!
//! Agents are nodes; every promise is an agent-to-agent edge labeled with its
//! behavior. Unresolvable behaviors appear as dashed "missing" ghost nodes.
//! The folding rules in `promise_tracker::fold` run between the walk and the
//! canvas: collectives and scratch groups are marked, parallel promises become
//! one line, and self promises stay inside their node.

use leptos::prelude::*;
use promise_tracker::fold::{fold, mark, FoldOptions, GroupSpec};
use promise_tracker::marks::Marks;
use promise_tracker::promise_graph::promise_graph;
use promise_tracker::Tracker;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use web_sys::CustomEvent;

use super::view_frame::ViewFrame;
use crate::utils::{generated_at, slugify};

// JS interop for the cytoscape glue in index.html
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = create_promise_graph)]
    fn js_create_promise_graph(container_id: &str, nodes: JsValue, edges: JsValue);

    #[wasm_bindgen(js_name = destroy_promise_graph)]
    fn js_destroy_promise_graph(container_id: &str);

    #[wasm_bindgen(js_name = focus_promise_graph)]
    fn js_focus_promise_graph(container_id: &str, agent: &str, behavior: &str);

    #[wasm_bindgen(js_name = reset_promise_graph)]
    fn js_reset_promise_graph(container_id: &str);

    #[wasm_bindgen(js_name = export_promise_graph)]
    fn js_export_promise_graph(container_id: &str, meta: JsValue) -> JsValue;

    #[wasm_bindgen(js_name = refit_promise_graph)]
    fn js_refit_promise_graph(container_id: &str);

    #[wasm_bindgen(js_name = selected_promise_graph_nodes)]
    fn js_selected_promise_graph_nodes(container_id: &str) -> JsValue;
}

/// Legend for the promise graph
/// What the exported page shows above the graph. The unsatisfied list is
/// passed rather than re-derived in JS: it comes from `resolution.is_satisfied()`
/// per want, which is not the same as filtering edges by `satisfied`.
#[derive(Serialize)]
struct GraphExportMeta {
    title: String,
    subtitle: String,
    unsatisfied: Vec<UnsatisfiedChip>,
}

/// One unsatisfied want on the status strip. `behavior` is what a click
/// looks for on the canvas; `label` is what the chip says in its place when
/// substituted values are marked.
#[derive(Clone, Serialize)]
struct UnsatisfiedChip {
    agent: String,
    behavior: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

impl UnsatisfiedChip {
    fn text(&self) -> String {
        format!(
            "{} ✗ {}",
            self.agent,
            self.label.as_deref().unwrap_or(&self.behavior)
        )
    }
}

#[component]
fn PromiseGraphLegend() -> impl IntoView {
    view! {
        <div class="promise-network-legend">
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color"
                    style="background-color: #1976D2;"
                ></span>
                <span>"Agent"</span>
            </div>
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color"
                    style="background: transparent; border: 2px dashed #C62828;"
                ></span>
                <span>"Missing behavior"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="color: #2E7D32;">"━━▶"</span>
                <span>"Promise (satisfied)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="color: #C62828;">"┅┅▶"</span>
                <span>"Promise (unsatisfied)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="color: #2E7D32;">"━━◆"</span>
                <span>"Condition"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="font-weight: bold;">"\u{00d7}n"</span>
                <span>"Bundle (tap to expand)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color"
                    style="background-color: #E3EAF2; border: 1px solid #78909C;"
                ></span>
                <span>"Collective"</span>
            </div>
        </div>
    }
}

/// PromiseGraphView component - displays an interactive edge-as-promise graph
/// with a status strip of unsatisfied wants.
#[component]
pub fn PromiseGraphView(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    sim_id: String,
    /// Which folding rules to apply before drawing
    #[prop(into)]
    fold_options: Signal<FoldOptions>,
    /// Whether substituted values are drawn wrapped in `<>`
    #[prop(into)]
    mark_bound: Signal<bool>,
    /// The reader's scratch groups, shared by every panel
    #[prop(into)]
    groups: Signal<Vec<GroupSpec>>,
    /// Replaces the scratch groups
    on_groups_change: Callback<Vec<GroupSpec>>,
    /// Which simulation this panel belongs to, for the export's header
    sim_label: String,
    #[prop(optional)] on_edge_select: Option<Callback<(String, String)>>,
) -> impl IntoView {
    // Generate a unique container ID for this instance
    let container_id = format!("promise-graph-{}", sim_id);
    let container_id_for_view = container_id.clone();
    let container_id_for_effect = container_id.clone();
    let container_id_for_cleanup = container_id.clone();
    let container_id_for_strip = container_id.clone();
    let container_id_for_reset = container_id.clone();
    let container_id_for_export = container_id.clone();
    let container_id_for_refit = container_id.clone();
    let container_id_for_group = container_id.clone();

    // Make a group of whatever is selected on the canvas. The name comes
    // from a prompt: one question, no form to build for a scratch grouping.
    let make_group = Callback::new(move |_: ()| {
        let selected: Vec<String> = serde_wasm_bindgen::from_value(
            js_selected_promise_graph_nodes(&container_id_for_group),
        )
        .unwrap_or_default();
        let win = web_sys::window().expect("no global window exists");
        if selected.is_empty() {
            let _ = win.alert_with_message(
                "Click the nodes to group first. Each click adds one; click the background to clear.",
            );
            return;
        }
        let Ok(Some(name)) = win.prompt_with_message_and_default(
            "Name for the group",
            &format!("group {}", groups.get_untracked().len() + 1),
        ) else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let mut next: Vec<GroupSpec> = groups
            .get_untracked()
            .into_iter()
            .filter(|g| g.name != name)
            .map(|g| GroupSpec {
                // A node is in one group at a time.
                members: g
                    .members
                    .into_iter()
                    .filter(|m| !selected.contains(m))
                    .collect(),
                ..g
            })
            .filter(|g| !g.members.is_empty())
            .collect();
        next.push(GroupSpec {
            name,
            members: selected,
            folded: false,
        });
        on_groups_change.run(next);
    });

    // The tree walk is the expensive half and does not depend on the display
    // options, so it stays in its own memo.
    let full_graph =
        Memo::new(move |_| tracker.get().map(|t| promise_graph(&t)).unwrap_or_default());

    // PartialEq on FoldedGraph makes this memo gate re-renders: a rule that
    // changes nothing in this panel's graph yields an equal value, so the
    // panel is never told to re-render and keeps its layout.
    // Where substituted values sit, read only while marking is on. The
    // switch arrives derived from the whole options value, so it is memoized
    // here: another option changing does not re-read the marks.
    let mark_bound = Memo::new(move |_| mark_bound.get());
    let marks = Memo::new(move |_| {
        mark_bound
            .get()
            .then(|| tracker.get().map(|t| Marks::new(&t)))
            .flatten()
    });

    // Marking only relabels: ids and behaviors are untouched, so the canvas
    // keeps its layout when the option is switched.
    let graph_data = Memo::new(move |_| {
        tracker
            .get()
            .map(|t| {
                let folded = fold(&full_graph.get(), &t, fold_options.get());
                match marks.get() {
                    Some(marks) => mark(folded, &marks),
                    None => folded,
                }
            })
            .unwrap_or_default()
    });

    // The status strip's chips: each want as its agent wrote it, and
    // nothing a condition of the same agent does with the same text.
    let unsatisfied_chips = move || -> Vec<UnsatisfiedChip> {
        let marks = marks.get();
        graph_data
            .get()
            .unsatisfied
            .into_iter()
            .map(|u| {
                let label = marks
                    .as_ref()
                    .map(|m| m.want(&u.agent, &u.behavior))
                    .filter(|label| *label != u.behavior);
                UnsatisfiedChip {
                    agent: u.agent,
                    behavior: u.behavior,
                    label,
                }
            })
            .collect()
    };

    // Determine the current state for conditional rendering
    let state = Memo::new(move |_| {
        let t = tracker.get();
        match t {
            None => "loading",
            Some(ref tracker) if tracker.is_empty() => "empty",
            Some(_) => {
                if graph_data.get().is_empty() {
                    "no_relationships"
                } else {
                    "ready"
                }
            }
        }
    });

    // Effect to render the graph when data changes AND state is ready
    Effect::new(move |_| {
        let current_state = state.get();
        let data = graph_data.get();
        let id = container_id_for_effect.clone();

        // Only render if state is "ready" (container exists)
        if current_state == "ready" && !data.is_empty() {
            // Use double requestAnimationFrame to ensure DOM is fully rendered
            // First RAF waits for Leptos to update the DOM
            // Second RAF waits for the browser to complete the render
            let _ = request_animation_frame(move || {
                let _ = request_animation_frame(move || {
                    let nodes = serde_wasm_bindgen::to_value(&data.nodes).unwrap_or(JsValue::NULL);
                    let edges = serde_wasm_bindgen::to_value(&data.edges).unwrap_or(JsValue::NULL);
                    js_create_promise_graph(&id, nodes, edges);
                });
            });
        }
    });

    // Edge tap in the graph dispatches a CustomEvent from JS; forward it to
    // the callback so the parent can navigate to the Detailed view.
    if let Some(cb) = on_edge_select {
        let my_id = container_id.clone();
        let handle = window_event_listener(
            leptos::ev::Custom::<CustomEvent>::new("pt-edge-select"),
            move |ev: CustomEvent| {
                let detail = ev.detail();
                let get = |key: &str| {
                    js_sys::Reflect::get(&detail, &JsValue::from_str(key))
                        .ok()
                        .and_then(|v| v.as_string())
                };
                if get("containerId").as_deref() == Some(my_id.as_str()) {
                    if let (Some(component), Some(behavior)) = (get("component"), get("behavior")) {
                        cb.run((component, behavior));
                    }
                }
            },
        );
        on_cleanup(move || handle.remove());
    }

    // Cleanup on unmount
    on_cleanup(move || {
        js_destroy_promise_graph(&container_id_for_cleanup);
    });

    let sim_for_title = sim_label.clone();
    let sim_for_file = sim_label.clone();
    let frame_title = Signal::derive(move || format!("Overview - Simulation {}", sim_for_title));
    let frame_subtitle = Signal::derive(move || {
        let opts = fold_options.get();
        let marking = mark_bound.get();
        let mut rules: Vec<&str> = vec![];
        if opts.collectives {
            rules.push("SuperAgents folded");
        }
        if opts.own_inside {
            rules.push("self promises inside");
        }
        if opts.bundle {
            rules.push("bundled");
        }
        let folded = if rules.is_empty() {
            "nothing folded".to_string()
        } else {
            rules.join(", ")
        };
        format!(
            "{}{} - generated {}",
            folded,
            if marking { ", bound values marked" } else { "" },
            generated_at()
        )
    });
    let frame_filename =
        Signal::derive(move || format!("promise-tracker-overview-{}", slugify(&sim_for_file)));

    // The page is assembled in the JS glue, where the graph runtime it has to
    // carry a copy of is in scope.
    let export = Callback::new(move |_: ()| {
        let meta = GraphExportMeta {
            title: frame_title.get_untracked(),
            subtitle: frame_subtitle.get_untracked(),
            unsatisfied: untrack(unsatisfied_chips),
        };
        let meta = serde_wasm_bindgen::to_value(&meta).ok()?;
        js_export_promise_graph(&container_id_for_export, meta)
            .as_string()
            .filter(|html| !html.is_empty())
    });

    // Cytoscape measures its container, so it has to be told once the frame has
    // resized it. The observer in the glue catches this too; going through both
    // keeps the graph from showing one unfitted frame on the way.
    let on_maximized = Callback::new(move |_: bool| {
        let id = container_id_for_refit.clone();
        let _ = request_animation_frame(move || {
            let _ = request_animation_frame(move || js_refit_promise_graph(&id));
        });
    });

    view! {
        <ViewFrame
            title=frame_title
            subtitle=frame_subtitle
            filename=frame_filename
            export=export
            on_maximized=on_maximized
            toolbar=move || {
                let id = container_id_for_reset.clone();
                view! {
                    <button
                        class="btn btn-sm btn-outline-primary"
                        style="flex-shrink: 0;"
                        title="Draw a scratch group around the selected nodes. Click nodes to select them."
                        on:click=move |_| make_group.run(())
                    >
                        "Group selected"
                    </button>
                    <button
                        class="btn btn-sm btn-outline-secondary"
                        style="flex-shrink: 0;"
                        title="Re-run the layout and fit the graph to the panel"
                        on:click=move |_| js_reset_promise_graph(&id)
                    >
                        "Reset view"
                    </button>
                }
            }
        >
        <div class="promise-network-container card-body">
            <Show when=move || state.get() == "loading">
                <div style="padding: 2rem; text-align: center; color: #666;">"Loading..."</div>
            </Show>

            <Show when=move || state.get() == "empty">
                <div style="padding: 2rem; text-align: center; color: #666;">
                    "No contracts defined. Add contracts to see relationships."
                </div>
            </Show>

            <Show when=move || state.get() == "no_relationships">
                <div style="padding: 2rem; text-align: center; color: #666;">
                    "No relationships found."
                </div>
            </Show>

            <Show when=move || state.get() == "ready">
                <PromiseGraphLegend />
                <Show when=move || !groups.get().is_empty()>
                    <div class="scratch-groups">
                        <span class="scratch-groups-head">"Scratch groups"</span>
                        {move || {
                            groups
                                .get()
                                .into_iter()
                                .map(|g| {
                                    let name = g.name.clone();
                                    let count = g.members.len();
                                    let folded = g.folded;
                                    let name_for_fold = name.clone();
                                    let name_for_drop = name.clone();
                                    let title = g.members.join(", ");
                                    view! {
                                        <span class="scratch-group" title=title>
                                            <span class="scratch-group-name">{name}</span>
                                            <span class="scratch-group-count">{format!("{} nodes", count)}</span>
                                            <button
                                                type="button"
                                                class="btn btn-sm btn-link"
                                                on:click=move |_| {
                                                    let next = groups
                                                        .get_untracked()
                                                        .into_iter()
                                                        .map(|mut g| {
                                                            if g.name == name_for_fold {
                                                                g.folded = !g.folded;
                                                            }
                                                            g
                                                        })
                                                        .collect();
                                                    on_groups_change.run(next);
                                                }
                                            >
                                                {if folded { "Open" } else { "Fold" }}
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-sm btn-link text-danger"
                                                on:click=move |_| {
                                                    let next = groups
                                                        .get_untracked()
                                                        .into_iter()
                                                        .filter(|g| g.name != name_for_drop)
                                                        .collect();
                                                    on_groups_change.run(next);
                                                }
                                            >
                                                "Ungroup"
                                            </button>
                                        </span>
                                    }
                                })
                                .collect::<Vec<_>>()
                        }}
                        <span class="scratch-groups-note">"Kept in this browser, not in the contract."</span>
                    </div>
                </Show>
                {
                    let strip_id = container_id_for_strip.clone();
                    move || {
                        let unsatisfied = unsatisfied_chips();
                        if unsatisfied.is_empty() {
                            view! {
                                <div style="margin-bottom: 0.5rem; color: #2E7D32; font-size: 0.85em;">
                                    "✓ All wants satisfied"
                                </div>
                            }
                            .into_any()
                        } else {
                            let strip_id = strip_id.clone();
                            view! {
                                <div style="display: flex; flex-wrap: wrap; gap: 0.25rem; margin-bottom: 0.5rem;">
                                    {unsatisfied
                                        .into_iter()
                                        .map(|u| {
                                            let id = strip_id.clone();
                                            let agent = u.agent.clone();
                                            let behavior = u.behavior.clone();
                                            let label = u.text();
                                            view! {
                                                <button
                                                    class="btn btn-sm btn-outline-danger"
                                                    on:click=move |_| {
                                                        js_focus_promise_graph(&id, &agent, &behavior)
                                                    }
                                                >
                                                    {label}
                                                </button>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </div>
                            }
                            .into_any()
                        }
                    }
                }
                <div id=container_id_for_view.clone() class="promise-graph-canvas"></div>
            </Show>
        </div>
        </ViewFrame>
    }
}

/// Helper function to schedule a callback using requestAnimationFrame
fn request_animation_frame<F>(f: F) -> Result<i32, JsValue>
where
    F: FnOnce() + 'static,
{
    let window = web_sys::window().expect("no global window exists");
    let closure = Closure::once_into_js(f);
    window.request_animation_frame(closure.as_ref().unchecked_ref())
}
