//! CycleBanner component - says when a simulation is built on a loop.
//!
//! Two different loops leave a simulation unable to answer, and neither says so
//! on its own:
//!
//! - Collectives that contain each other. Each makes a template of the next, so
//!   none of them stands as a working agent and everything they hold is simply
//!   absent. The contract parses and every document loads.
//! - A promise that depends on itself, however far around. The resolver's guard
//!   returns an empty answer, which draws exactly like nobody having promised
//!   the behavior at all.
//!
//! The contract is valid either way, the views render either way, and the
//! reason the simulation cannot answer is nowhere on the page. This is that
//! reason.

use leptos::prelude::*;
use promise_tracker::Tracker;

/// How many loops to name before summarizing the rest.
const SHOWN: usize = 5;

/// One loop written the way it reads, closing back on where it started.
fn as_chain(cycle: &[String]) -> String {
    let mut chain: Vec<&str> = cycle.iter().map(|g| g.as_str()).collect();
    if let Some(first) = cycle.first() {
        chain.push(first);
    }
    chain.join(" \u{2192} ")
}

/// The listed loops, and how many were left off the end.
#[component]
fn CycleList(#[prop(into)] cycles: Signal<Vec<Vec<String>>>) -> impl IntoView {
    // Held apart from the view because the macro reads a bare `>` in an
    // attribute as the end of the tag.
    let elided = Memo::new(move |_| cycles.get().len().saturating_sub(SHOWN));

    view! {
        <ul class="mb-0" style="padding-left: 1.25rem;">
            {move || {
                cycles
                    .get()
                    .iter()
                    .take(SHOWN)
                    .map(|cycle| {
                        view! { <li style="font-family: monospace;">{as_chain(cycle)}</li> }
                    })
                    .collect::<Vec<_>>()
            }}
        </ul>
        <Show when=move || elided.get() != 0>
            <div style="margin-top: 0.25rem;">
                {move || format!("and {} more", elided.get())}
            </div>
        </Show>
    }
}

/// Warning strip for a simulation built on a loop.
///
/// Renders nothing when there is no loop, so it can sit unconditionally at the
/// top of any panel.
#[component]
pub fn CycleBanner(#[prop(into)] tracker: Signal<Option<Tracker>>) -> impl IntoView {
    // Both walks cover the whole contract, so they are memoized against the
    // tracker rather than run per render.
    let membership = Memo::new(move |_| {
        tracker
            .get()
            .map(|t| t.membership_cycles())
            .unwrap_or_default()
    });
    let dependency = Memo::new(move |_| {
        tracker
            .get()
            .map(|t| t.dependency_cycles())
            .unwrap_or_default()
    });

    // Both blocks are alerts of the same weight. They differ in what is wrong
    // and so in what fixes it, not in how much it matters: either way the
    // simulation's answer cannot be trusted, and neither is something to
    // proceed past. The headings are what tell them apart.
    view! {
        // Membership first, because it is the one that explains agents being
        // absent rather than merely unsatisfied.
        <Show when=move || !membership.get().is_empty()>
            <div class="alert alert-danger py-2 px-2 mb-2" role="alert" style="font-size: 0.85em;">
                <div style="font-weight: bold;">
                    {move || {
                        let n = membership.get().len();
                        if n == 1 {
                            "Collectives contain each other".to_string()
                        } else {
                            format!("{} sets of collectives contain each other", n)
                        }
                    }}
                </div>
                <div style="margin-bottom: 0.25rem;">
                    "A collective folded into another describes it rather than being it, \
                     so each of these makes a template of the next and none of them stands \
                     as an agent. Everything they hold is missing from this simulation."
                </div>
                <CycleList cycles=membership />
            </div>
        </Show>

        <Show when=move || !dependency.get().is_empty()>
            <div class="alert alert-danger py-2 px-2 mb-2" role="alert" style="font-size: 0.85em;">
                <div style="font-weight: bold;">
                    {move || {
                        let n = dependency.get().len();
                        if n == 1 {
                            "Circular dependency".to_string()
                        } else {
                            format!("{} circular dependencies", n)
                        }
                    }}
                </div>
                <div style="margin-bottom: 0.25rem;">
                    "These promises depend on themselves, so nothing can keep them. \
                     Resolution stops at the loop, and every behavior below it reads \
                     as unpromised."
                </div>
                <CycleList cycles=dependency />
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_closes_on_where_it_started() {
        assert_eq!(
            as_chain(&["b1".to_string(), "b2".to_string()]),
            "b1 \u{2192} b2 \u{2192} b1"
        );
    }

    #[test]
    fn a_promise_depending_on_itself_reads_as_one_step() {
        assert_eq!(as_chain(&["b1".to_string()]), "b1 \u{2192} b1");
    }
}
