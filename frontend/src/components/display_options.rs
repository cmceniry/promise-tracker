use leptos::prelude::*;

/// Display option toggles for the contract views.
///
/// Icon-only buttons, so each one carries its meaning and its current state in
/// the tooltip. The bar matches `SimulationControls` above it.
#[component]
pub fn DisplayOptions(
    show_self_promises: ReadSignal<bool>,
    on_toggle_self_promises: Callback<()>,
) -> impl IntoView {
    view! {
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; padding: 0.75rem; background: #f8f9fa; border-radius: 4px;">
            <div style="display: flex; align-items: center; gap: 0.5rem;">
                <span style="font-weight: bold;">"Display:"</span>
                <span style="color: #666;">
                    {move || {
                        if show_self_promises.get() {
                            "showing self-fulfilling promises"
                        } else {
                            "self-fulfilling promises hidden"
                        }
                    }}
                </span>
            </div>
            <div style="display: flex; gap: 0.5rem;">
                <button
                    type="button"
                    class=move || {
                        if show_self_promises.get() {
                            "btn btn-sm btn-primary"
                        } else {
                            "btn btn-sm btn-outline-secondary"
                        }
                    }
                    aria-pressed=move || if show_self_promises.get() { "true" } else { "false" }
                    aria-label="Display self-fulfilling promises"
                    title=move || {
                        if show_self_promises.get() {
                            "Self-fulfilling promises shown — click to hide promises an agent makes to itself"
                        } else {
                            "Self-fulfilling promises hidden — click to show promises an agent makes to itself"
                        }
                    }
                    on:click=move |_| on_toggle_self_promises.run(())
                >
                    // A loop: the promise comes back to the agent that made it.
                    // Struck through when the promises are hidden.
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="16"
                        height="16"
                        fill="currentColor"
                        viewBox="0 0 16 16"
                    >
                        <path
                            d="M8 3a5 5 0 1 0 5 5"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                        ></path>
                        <path d="M8 0.5 5 3 8 5.5z"></path>
                        <Show when=move || !show_self_promises.get()>
                            <path
                                d="M2 14 14 2"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.5"
                            ></path>
                        </Show>
                    </svg>
                </button>
            </div>
        </div>
    }
}
