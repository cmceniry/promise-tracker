//! PromiseGraphView component - edge-as-promise graph rendered with Cytoscape + dagre.
//!
//! Agents are nodes; every promise is an agent-to-agent edge labeled with its
//! behavior. Unresolvable behaviors appear as dashed "missing" ghost nodes.

use leptos::prelude::*;
use promise_tracker::promise_graph::{promise_graph, UnsatisfiedWant};
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
}

/// Legend for the promise graph
/// What the exported page shows above the graph. The unsatisfied list is
/// passed rather than re-derived in JS: it comes from `resolution.is_satisfied()`
/// per want, which is not the same as filtering edges by `satisfied`.
#[derive(Serialize)]
struct GraphExportMeta {
    title: String,
    subtitle: String,
    unsatisfied: Vec<UnsatisfiedWant>,
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
                <span style="font-weight: bold;">"n promises"</span>
                <span>"Bundle (tap to expand)"</span>
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
    #[prop(into)] show_self_promises: Signal<bool>,
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

    // The tree walk is the expensive half and does not depend on the display
    // options, so it stays in its own memo.
    let full_graph =
        Memo::new(move |_| tracker.get().map(|t| promise_graph(&t)).unwrap_or_default());

    // PartialEq on PromiseGraphData makes this memo gate re-renders: filtering
    // a graph that has no self promises returns an equal value, so panels
    // without any are never told to re-render and keep their layout.
    let graph_data = Memo::new(move |_| {
        let data = full_graph.get();
        if show_self_promises.get() {
            data
        } else {
            data.without_self_promises()
        }
    });

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
        let self_state = if show_self_promises.get() {
            "self-fulfilling promises shown"
        } else {
            "self-fulfilling promises hidden"
        };
        format!("{} - generated {}", self_state, generated_at())
    });
    let frame_filename =
        Signal::derive(move || format!("promise-tracker-overview-{}", slugify(&sim_for_file)));

    // The page is assembled in the JS glue, where the graph runtime it has to
    // carry a copy of is in scope.
    let export = Callback::new(move |_: ()| {
        let meta = GraphExportMeta {
            title: frame_title.get_untracked(),
            subtitle: frame_subtitle.get_untracked(),
            unsatisfied: graph_data.get_untracked().unsatisfied,
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
                {
                    let strip_id = container_id_for_strip.clone();
                    move || {
                        let unsatisfied = graph_data.get().unsatisfied;
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
                                            let label = format!("{} ✗ {}", u.agent, u.behavior);
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
