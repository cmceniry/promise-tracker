use leptos::prelude::*;

/// Simulation management controls component
#[component]
pub fn SimulationControls(
    simulations: ReadSignal<Vec<String>>,
    on_add: Callback<()>,
    on_remove: Callback<()>,
) -> impl IntoView {
    // Check if we're at maximum (5 simulations)
    let at_max = move || simulations.get().len() >= 5;

    // Check if we're at minimum (1 simulation)
    let at_min = move || simulations.get().len() <= 1;

    view! {
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; padding: 0.75rem; background: #f8f9fa; border-radius: 4px;">
            <div style="display: flex; align-items: center; gap: 0.5rem;">
                <span style="font-weight: bold;">"Simulations:"</span>
                <span style="color: #666;">
                    {move || format!("{} active", simulations.get().len())}
                </span>
                <div style="display: flex; gap: 0.25rem;">
                    {move || {
                        simulations
                            .get()
                            .into_iter()
                            .map(|sim| {
                                view! {
                                    <span
                                        class="badge bg-secondary"
                                        style="font-size: 0.9em;"
                                    >
                                        {sim}
                                    </span>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </div>
            </div>
            <div style="display: flex; gap: 0.5rem;">
                <button
                    class="btn btn-success btn-sm"
                    on:click=move |_| on_add.run(())
                    disabled=move || at_max()
                    title="Add simulation (max 5)"
                >
                    "+"
                </button>
                <button
                    class="btn btn-danger btn-sm"
                    on:click=move |_| on_remove.run(())
                    disabled=move || at_min()
                    title="Remove last simulation (min 1)"
                >
                    "-"
                </button>
            </div>
        </div>
    }
}
