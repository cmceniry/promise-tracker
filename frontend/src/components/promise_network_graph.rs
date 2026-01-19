//! PromiseNetworkGraph component - Interactive force-directed graph visualization.

use leptos::prelude::*;
use promise_tracker::network_diagram::{network_diagram, GraphData};
use promise_tracker::Tracker;
use wasm_bindgen::prelude::*;

// JS interop for force graph functions
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = create_force_graph)]
    fn js_create_force_graph(container_id: &str, nodes: JsValue, links: JsValue);

    #[wasm_bindgen(js_name = destroy_force_graph)]
    fn js_destroy_force_graph(container_id: &str);
}

/// Legend component for the network graph
#[component]
fn NetworkLegend() -> impl IntoView {
    view! {
        <div class="promise-network-legend">
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color circle"
                    style="background-color: #1976D2;"
                ></span>
                <span>"Component"</span>
            </div>
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color circle"
                    style="background-color: #4CAF50;"
                ></span>
                <span>"Behavior (satisfied)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span
                    class="promise-network-legend-color circle"
                    style="background-color: #C62828;"
                ></span>
                <span>"Behavior (unsatisfied)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="color: #4CAF50;">"━━"</span>
                <span>"Relationship (satisfied)"</span>
            </div>
            <div class="promise-network-legend-item">
                <span style="color: #C62828;">"┅┅"</span>
                <span>"Relationship (unsatisfied)"</span>
            </div>
        </div>
    }
}

/// Convert GraphData to JsValue for passing to JavaScript
fn graph_data_to_js(data: &GraphData) -> (JsValue, JsValue) {
    let nodes = serde_wasm_bindgen::to_value(&data.nodes).unwrap_or(JsValue::NULL);
    let links = serde_wasm_bindgen::to_value(&data.links).unwrap_or(JsValue::NULL);
    (nodes, links)
}

/// PromiseNetworkGraph component - displays an interactive force-directed graph
/// of promise relationships between components and behaviors.
#[component]
pub fn PromiseNetworkGraph(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    sim_id: String,
) -> impl IntoView {
    // Generate a unique container ID for this instance
    let container_id = format!("graph-container-{}", sim_id);
    let container_id_clone = container_id.clone();
    let container_id_for_effect = container_id.clone();
    let container_id_for_cleanup = container_id.clone();

    // Track the current graph data to detect changes
    let graph_data = Memo::new(move |_| {
        tracker
            .get()
            .map(|t| network_diagram(&t))
            .unwrap_or_default()
    });

    // Determine the current state for conditional rendering
    let state = Memo::new(move |_| {
        let t = tracker.get();
        match t {
            None => "loading",
            Some(ref tracker) if tracker.is_empty() => "empty",
            Some(_) => {
                let data = graph_data.get();
                if data.is_empty() {
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
                    let (nodes, links) = graph_data_to_js(&data);
                    js_create_force_graph(&id, nodes, links);
                });
            });
        }
    });

    // Cleanup on unmount
    on_cleanup(move || {
        js_destroy_force_graph(&container_id_for_cleanup);
    });

    view! {
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
                <NetworkLegend />
                <div
                    id=container_id_clone.clone()
                    style="width: 100%; height: 400px; min-height: 300px;"
                ></div>
            </Show>
        </div>
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
