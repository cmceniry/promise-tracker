//! SimplifyPanel component - the folding rules for the overview graph.
//!
//! One row per rule, each saying what it takes off the canvas on the current
//! contract, and a line of what is left drawn. The counts come from
//! `promise_tracker::fold` over the main tracker, so they describe every
//! contract loaded rather than one simulation's panel.

use leptos::prelude::*;
use promise_tracker::fold::{FoldOptions, FoldStats};

/// Where the threshold control starts and stops.
const THRESHOLD_MIN: usize = 2;
const THRESHOLD_MAX: usize = 200;

#[component]
pub fn SimplifyPanel(
    fold_options: ReadSignal<FoldOptions>,
    set_fold_options: WriteSignal<FoldOptions>,
    /// The display toggle that rule four is bound to
    show_self_promises: ReadSignal<bool>,
    on_toggle_self_promises: Callback<()>,
    #[prop(into)] stats: Signal<Option<FoldStats>>,
) -> impl IntoView {
    // The threshold the slider last had, kept while the hub rule is off so
    // turning it back on restores it.
    let (remembered_threshold, set_remembered_threshold) = signal(
        fold_options
            .get_untracked()
            .hub_threshold
            .unwrap_or_else(|| FoldOptions::default().hub_threshold.unwrap_or(15)),
    );

    let hubs_on = move || fold_options.get().hub_threshold.is_some();
    let threshold = move || {
        fold_options
            .get()
            .hub_threshold
            .unwrap_or_else(|| remembered_threshold.get())
    };

    let toggle_hubs = move |_| {
        set_fold_options.update(|o| {
            o.hub_threshold = if o.hub_threshold.is_some() {
                None
            } else {
                Some(remembered_threshold.get_untracked())
            };
        });
    };
    let on_threshold = move |ev: web_sys::Event| {
        let raw = event_target_value(&ev);
        let Ok(n) = raw.trim().parse::<usize>() else {
            return;
        };
        let n = n.clamp(THRESHOLD_MIN, THRESHOLD_MAX);
        set_remembered_threshold.set(n);
        set_fold_options.update(|o| {
            if o.hub_threshold.is_some() {
                o.hub_threshold = Some(n);
            }
        });
    };
    let toggle_bundle = move |_| set_fold_options.update(|o| o.bundle = !o.bundle);
    let toggle_collectives = move |_| set_fold_options.update(|o| o.collectives = !o.collectives);

    let hub_line = move || {
        let Some(st) = stats.get() else {
            return String::new();
        };
        if !hubs_on() {
            return format!(
                "Off. Top providers: {}.",
                st.candidates
                    .iter()
                    .take(3)
                    .map(|(name, n)| format!("{} {}", name, n))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if st.hubs.is_empty() {
            let top = st
                .candidates
                .first()
                .map(|(name, n)| format!(" The most leaned on is {} at {}.", name, n))
                .unwrap_or_default();
            format!("No provider reaches {} promises in.{}", threshold(), top)
        } else {
            format!(
                "{} line{} folded into badges: {}.",
                st.hub_lines_folded,
                if st.hub_lines_folded == 1 { "" } else { "s" },
                st.hubs
                    .iter()
                    .map(|(name, n)| format!("{} {}", name, n))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    };

    view! {
        <div class="simplify-panel">
            <div class="simplify-head">
                <span style="font-weight: bold;">"Simplify"</span>
                <span class="simplify-summary">
                    {move || {
                        stats
                            .get()
                            .map(|st| {
                                format!(
                                    "{} promises drawn as {} nodes, {} lines, {} badges",
                                    st.promises,
                                    st.nodes_drawn,
                                    st.lines_drawn,
                                    st.badges_drawn,
                                )
                            })
                            .unwrap_or_default()
                    }}
                </span>
            </div>
            <div class="simplify-rules">
                <label class="simplify-rule">
                    <input
                        type="checkbox"
                        prop:checked=move || fold_options.get().collectives
                        on:change=toggle_collectives
                    />
                    <span>
                        <span class="simplify-rule-name">"Mark collectives"</span>
                        <span class="simplify-rule-note">
                            "A SuperAgent draws as one node with its member count. The tracker folds its members before resolving, so there is no unfolded graph: this only changes the marking."
                        </span>
                    </span>
                </label>
                <label class="simplify-rule">
                    <input
                        type="checkbox"
                        prop:checked=hubs_on
                        on:change=toggle_hubs
                    />
                    <span>
                        <span class="simplify-rule-name">
                            "Hubs become badges at "
                            <input
                                type="number"
                                class="simplify-threshold"
                                min=THRESHOLD_MIN
                                max=THRESHOLD_MAX
                                prop:value=move || threshold().to_string()
                                on:change=on_threshold
                                on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                                disabled=move || !hubs_on()
                                aria-label="Promises leaned on before a provider becomes a hub"
                            />
                            " promises in"
                        </span>
                        <span class="simplify-rule-note">{hub_line}</span>
                    </span>
                </label>
                <label class="simplify-rule">
                    <input
                        type="checkbox"
                        prop:checked=move || fold_options.get().bundle
                        on:change=toggle_bundle
                    />
                    <span>
                        <span class="simplify-rule-name">"Bundle parallel promises"</span>
                        <span class="simplify-rule-note">
                            {move || {
                                stats
                                    .get()
                                    .map(|st| {
                                        if fold_options.get().bundle {
                                            format!(
                                                "One line per pair, labelled \u{00d7}N; tap it to see the promises. {} line{} fewer.",
                                                st.bundled_away,
                                                if st.bundled_away == 1 { "" } else { "s" },
                                            )
                                        } else {
                                            "Off. Every promise is its own line.".to_string()
                                        }
                                    })
                                    .unwrap_or_default()
                            }}
                        </span>
                    </span>
                </label>
                <label class="simplify-rule">
                    <input
                        type="checkbox"
                        prop:checked=move || !show_self_promises.get()
                        on:change=move |_| on_toggle_self_promises.run(())
                    />
                    <span>
                        <span class="simplify-rule-name">"Self promises stay inside the node"</span>
                        <span class="simplify-rule-note">
                            {move || {
                                stats
                                    .get()
                                    .map(|st| {
                                        if show_self_promises.get() {
                                            "Off. A promise an agent makes to itself is drawn as a loop. This is the same switch as the display toggle above.".to_string()
                                        } else {
                                            format!(
                                                "{} promise{} an agent meets itself, counted on the node as +N own.",
                                                st.own_folded,
                                                if st.own_folded == 1 { "" } else { "s" },
                                            )
                                        }
                                    })
                                    .unwrap_or_default()
                            }}
                        </span>
                    </span>
                </label>
            </div>
            <div class="simplify-foot">
                "Never folded away: a promise that is not held keeps its line or turns its badge red, and a missing behavior is never a hub."
            </div>
        </div>
    }
}
