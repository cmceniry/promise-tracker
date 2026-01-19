use leptos::prelude::*;
use promise_tracker::diagram::diagram;
use promise_tracker::Tracker;
use wasm_bindgen::prelude::*;

// JS interop for Mermaid rendering
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = render_mermaid, catch)]
    async fn js_render_mermaid(source: &str) -> Result<JsValue, JsValue>;
}

/// Mermaid rendering state
#[derive(Clone, PartialEq)]
enum MermaidState {
    Loading,
    Success(String),
    Error(String),
    Empty,
}

/// Component for rendering Mermaid diagrams.
///
/// Renders Mermaid DSL source to SVG using the Mermaid.js library.
#[component]
fn Mermaid(#[prop(into)] source: Signal<String>) -> impl IntoView {
    // Track the rendering state
    let (state, set_state) = signal(MermaidState::Loading);

    // Store the last source to show in error details
    let (last_source, set_last_source) = signal(String::new());

    // Effect to render when source changes
    Effect::new(move |_| {
        let src = source.get();
        set_last_source.set(src.clone());

        if src.is_empty() {
            set_state.set(MermaidState::Empty);
            return;
        }

        set_state.set(MermaidState::Loading);

        // Spawn async task to render the diagram
        leptos::task::spawn_local(async move {
            match js_render_mermaid(&src).await {
                Ok(result) => {
                    let svg = result.as_string().unwrap_or_default();
                    if svg.is_empty() {
                        set_state.set(MermaidState::Empty);
                    } else {
                        set_state.set(MermaidState::Success(svg));
                    }
                }
                Err(err) => {
                    let error_msg = err
                        .as_string()
                        .or_else(|| {
                            js_sys::Reflect::get(&err, &JsValue::from_str("message"))
                                .ok()
                                .and_then(|v| v.as_string())
                        })
                        .unwrap_or_else(|| "Unknown error".to_string());
                    set_state.set(MermaidState::Error(error_msg));
                }
            }
        });
    });

    view! {
        <div class="mermaid-container">
            {move || match state.get() {
                MermaidState::Loading => {
                    view! {
                        <div
                            style="overflow: auto; padding: 1rem; text-align: center;"
                            class="text-muted"
                        >
                            <div class="spinner-border spinner-border-sm me-2" role="status">
                                <span class="visually-hidden">"Loading..."</span>
                            </div>
                            "Rendering diagram..."
                        </div>
                    }
                        .into_any()
                }
                MermaidState::Empty => {
                    view! {
                        <div
                            style="overflow: auto; padding: 1rem; text-align: center;"
                            class="text-muted"
                        >
                            "No diagram to display"
                        </div>
                    }
                        .into_any()
                }
                MermaidState::Error(ref error) => {
                    let error_msg = error.clone();
                    let source_for_details = last_source.get();
                    view! {
                        <div style="overflow: auto; padding: 1rem;">
                            <div style="color: #dc3545; margin-bottom: 0.5rem;">
                                "Error rendering diagram:"
                            </div>
                            <div style="font-size: 0.9em; color: #666;">{error_msg}</div>
                            <details style="margin-top: 1rem;">
                                <summary style="cursor: pointer;">"Show chart source"</summary>
                                <pre style="background: #f5f5f5; padding: 0.5rem; margin-top: 0.5rem; overflow: auto; font-size: 0.8em;">
                                    {source_for_details}
                                </pre>
                            </details>
                        </div>
                    }
                        .into_any()
                }
                MermaidState::Success(ref svg) => {
                    let svg_html = svg.clone();
                    view! {
                        <div
                            style="overflow: auto; width: 100%; min-height: 200px;"
                            inner_html=svg_html
                        ></div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// Sequence diagram component showing promise resolution flow.
///
/// Generates a Mermaid sequence diagram DSL from the tracker's resolution data
/// and displays it using the Mermaid component.
#[component]
pub fn ContractGraph(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    selected_component: ReadSignal<String>,
    selected_behavior: ReadSignal<String>,
) -> impl IntoView {
    // Generate the diagram DSL reactively
    let diagram_source = Memo::new(move |_| {
        let tracker_opt = tracker.get();
        let component = selected_component.get();
        let behavior = selected_behavior.get();

        // Handle edge cases - return placeholder diagrams
        let Some(pt) = tracker_opt else {
            return placeholder_diagram("No tracker available");
        };

        if pt.is_empty() {
            return placeholder_diagram("Add components to this simulation");
        }

        if component == "---" {
            return placeholder_diagram("Select a component");
        }

        if !pt.has_agent(component.clone()) {
            return placeholder_diagram("Select a component in this simulation");
        }

        if pt.get_agent_wants(component.clone()).is_empty() {
            return placeholder_diagram("Select a component with wants");
        }

        if behavior == "---" {
            return placeholder_diagram("Select a behavior");
        }

        if !pt.has_behavior(behavior.clone()) {
            return placeholder_diagram("Select a valid behavior");
        }

        // Generate the actual diagram
        let resolution = pt.resolve(&behavior);
        diagram(&component, &behavior, &resolution)
    });

    let source_signal = Signal::derive(move || diagram_source.get());

    view! {
        <div class="contract-graph">
            <Mermaid source=source_signal />
        </div>
    }
}

/// Generate a placeholder sequence diagram with a message.
fn placeholder_diagram(message: &str) -> String {
    format!("sequenceDiagram\n    Note over System: {}", message)
}
