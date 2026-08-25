use leptos::prelude::*;
use promise_tracker::resolve::{Offer, Resolution};
use promise_tracker::Tracker;

/// One line of the resolution tree, with whatever hangs under it.
///
/// The shape is worked out as plain data first so that what the view shows can
/// be reasoned about and tested without a browser.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    class: &'static str,
    label: String,
    children: Vec<Row>,
}

impl Row {
    fn new(class: &'static str, label: String, children: Vec<Row>) -> Row {
        Row {
            class,
            label,
            children,
        }
    }

    fn leaf(class: &'static str, label: String) -> Row {
        Row::new(class, label, vec![])
    }
}

/// Whether every offer for a behavior comes from the agent that needs it.
///
/// When that is so and self promises are hidden, the behavior itself has
/// nothing left to show: naming the need while its only answer is hidden would
/// be showing the self-fulfilling promise that was turned off.
fn only_self_offers(
    requirer: &str,
    satisfying: &[Offer],
    unsatisfying: &[Offer],
    show_self: bool,
) -> bool {
    !show_self
        && satisfying
            .iter()
            .chain(unsatisfying.iter())
            .all(|offer| offer.get_agent_name() == requirer)
}

/// The rows contributed by a single offer.
///
/// A hidden self-fulfilling promise contributes its conditions to the caller's
/// list instead of a row of its own, so whatever the hidden provider needs from
/// other agents stays visible.
fn offer_rows(requirer: &str, offer: &Offer, is_satisfied: bool, show_self: bool) -> Vec<Row> {
    let agent_name = offer.get_agent_name().to_string();
    let conditions = offer.get_resolved_conditions();

    if !show_self && agent_name == requirer {
        return conditions
            .iter()
            .flat_map(|c| resolution_rows(&agent_name, c, show_self))
            .collect();
    }

    // An offer that satisfies nothing and asks for nothing is an error case
    if !is_satisfied && conditions.is_empty() {
        return vec![Row::leaf(
            "contract-text-error",
            format!("ERROR: {}", agent_name),
        )];
    }

    let class = if is_satisfied {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    let label = if is_satisfied {
        format!("OPTION: {}", agent_name)
    } else {
        format!("POSSIBLE: {}", agent_name)
    };

    // The requirer one level down is this offer's agent: it is the one that
    // needs the conditions.
    let children: Vec<Row> = conditions
        .iter()
        .flat_map(|c| resolution_rows(&agent_name, c, show_self))
        .collect();

    vec![Row::new(class, label, children)]
}

/// The rows for every offer, satisfying ones first.
fn option_rows(
    requirer: &str,
    satisfying: &[Offer],
    unsatisfying: &[Offer],
    show_self: bool,
) -> Vec<Row> {
    let mut options: Vec<Row> = satisfying
        .iter()
        .flat_map(|offer| offer_rows(requirer, offer, true, show_self))
        .collect();

    options.extend(
        unsatisfying
            .iter()
            .flat_map(|offer| offer_rows(requirer, offer, false, show_self)),
    );

    options
}

/// The rows for a behavior and whoever offers it.
///
/// Yields nothing when the behavior was only ever self-provided; whatever that
/// hidden provider needed from other agents rises to the caller's list.
fn resolution_rows(requirer: &str, resolution: &Resolution, show_self: bool) -> Vec<Row> {
    let behavior_name = resolution.get_behavior_name().to_string();
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return vec![Row::leaf(
            "contract-text-possible",
            format!("{} UNSATISFIED", behavior_name),
        )];
    }

    let options = option_rows(requirer, satisfying, unsatisfying, show_self);

    if only_self_offers(requirer, satisfying, unsatisfying, show_self) {
        // Drop the behavior itself, hand its downstream needs upwards
        return options;
    }

    // Determine the CSS class based on whether there are any satisfying offers
    let class = if !satisfying.is_empty() {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    vec![Row::new(class, behavior_name, options)]
}

/// The rows for the root resolution, `component --> behavior`.
///
/// Empty when the selected behavior is entirely self-provided and self promises
/// are hidden; the caller says so instead of showing a bare heading.
fn contract_rows(component: &str, resolution: &Resolution, show_self: bool) -> Vec<Row> {
    let behavior_name = resolution.get_behavior_name().to_string();
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return vec![Row::leaf(
            "contract-text-possible",
            format!("{} --> {} UNSATISFIED", component, behavior_name),
        )];
    }

    let options = option_rows(component, satisfying, unsatisfying, show_self);

    if options.is_empty() {
        return vec![];
    }

    // Determine the CSS class based on whether there are any satisfying offers
    let class = if !satisfying.is_empty() {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    // The heading stays even when everything under it was rearranged: it is
    // the question being asked, not a promise.
    vec![Row::new(
        class,
        format!("{} --> {}", component, behavior_name),
        options,
    )]
}

/// Renders one row and its children as nested list items.
fn render_row(row: Row) -> AnyView {
    let Row {
        class,
        label,
        children,
    } = row;

    if children.is_empty() {
        return view! {
            <li class=class>{label}</li>
        }
        .into_any();
    }

    let children: Vec<AnyView> = children.into_iter().map(render_row).collect();

    view! {
        <li class=class>
            {label}
            <ul class="contract-text-list">{children}</ul>
        </li>
    }
    .into_any()
}

/// Displays promise resolution as hierarchical text/list view.
#[component]
pub fn ContractText(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    selected_component: ReadSignal<String>,
    selected_behavior: ReadSignal<String>,
    #[prop(into)] show_self_promises: Signal<bool>,
) -> impl IntoView {
    let content = move || {
        let tracker_opt = tracker.get();
        let component = selected_component.get();
        let behavior = selected_behavior.get();

        // Handle edge cases with placeholder messages
        let Some(pt) = tracker_opt else {
            return view! {
                <div class="text-muted p-3">"No tracker available"</div>
            }
            .into_any();
        };

        if pt.is_empty() {
            return view! {
                <div class="text-muted p-3">"Add components to this simulation"</div>
            }
            .into_any();
        }

        if component == "---" {
            return view! {
                <div class="text-muted p-3">"Select a component"</div>
            }
            .into_any();
        }

        if !pt.has_agent(component.clone()) {
            return view! {
                <div class="text-muted p-3">"Select a component in this simulation"</div>
            }
            .into_any();
        }

        if pt.get_agent_wants(component.clone()).is_empty() {
            return view! {
                <div class="text-muted p-3">"Select a component with wants"</div>
            }
            .into_any();
        }

        if behavior == "---" {
            return view! {
                <div class="text-muted p-3">"Select a behavior"</div>
            }
            .into_any();
        }

        if !pt.has_behavior(behavior.clone()) {
            return view! {
                <div class="text-muted p-3">"Select a valid behavior"</div>
            }
            .into_any();
        }

        // Resolve the behavior and render the result
        let resolution = pt.resolve(&behavior);
        let rows = contract_rows(&component, &resolution, show_self_promises.get());

        if rows.is_empty() {
            return view! {
                <div class="text-muted p-3">
                    "Nothing to show: this behavior is self-provided, and self-fulfilling promises are hidden"
                </div>
            }
            .into_any();
        }

        let contract_text: Vec<AnyView> = rows.into_iter().map(render_row).collect();

        view! {
            <div class="card">
                <div class="card-body contract-text-card">
                    <ul class="contract-text-list">{contract_text}</ul>
                </div>
            </div>
        }
        .into_any()
    };

    view! { <div class="contract-text">{content}</div> }
}

#[cfg(test)]
mod tests {
    use super::*;
    use promise_tracker::components::{Agent, Behavior};

    /// The rows as indented text, the way the view nests them.
    fn outline(rows: &[Row]) -> String {
        fn walk(rows: &[Row], depth: usize, out: &mut String) {
            for row in rows {
                out.push_str(&"    ".repeat(depth));
                out.push_str(&row.label);
                out.push('\n');
                walk(&row.children, depth + 1, out);
            }
        }
        let mut out = String::new();
        walk(rows, 0, &mut out);
        out
    }

    fn behavior_with(name: &str, conditions: &[&str]) -> Behavior {
        Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
    }

    /// b wants pa; a offers it but needs paa, which a provides for itself,
    /// and that in turn needs pc from c.
    fn self_condition_tracker() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("b").with_wants(vec![Behavior::build("pa")]));
        tracker.add_agent(Agent::build("a").with_provides(vec![
            behavior_with("pa", &["paa"]),
            behavior_with("paa", &["pc"]),
        ]));
        tracker.add_agent(Agent::build("c").with_provides(vec![Behavior::build("pc")]));
        tracker
    }

    #[test]
    fn shows_the_self_fulfilled_behavior_when_enabled() {
        let tracker = self_condition_tracker();
        let rows = contract_rows("b", &tracker.resolve("pa"), true);

        assert_eq!(
            outline(&rows),
            "b --> pa\n\
             \x20   OPTION: a\n\
             \x20       paa\n\
             \x20           OPTION: a\n\
             \x20               pc\n\
             \x20                   OPTION: c\n"
        );
    }

    #[test]
    fn hides_the_self_fulfilled_behavior_and_keeps_what_is_downstream() {
        let tracker = self_condition_tracker();
        let rows = contract_rows("b", &tracker.resolve("pa"), false);

        // `paa` is what a promises itself, so neither it nor its OPTION row is
        // shown; pc, which a needs from c, still is.
        assert_eq!(
            outline(&rows),
            "b --> pa\n\
             \x20   OPTION: a\n\
             \x20       pc\n\
             \x20           OPTION: c\n"
        );
    }

    #[test]
    fn hidden_self_promise_with_no_conditions_leaves_nothing_behind() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a").with_provides(vec![behavior_with("pa", &["paa"])]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("paa")]));
        // a provides paa for itself too, unconditionally.
        tracker.add_agent(Agent::build("a").with_provides(vec![Behavior::build("paa")]));

        let rows = contract_rows("b", &tracker.resolve("pa"), false);

        // paa survives because a2 offers it; only a's own offer disappears.
        assert_eq!(
            outline(&rows),
            "b --> pa\n\
             \x20   OPTION: a\n\
             \x20       paa\n\
             \x20           OPTION: a2\n"
        );
    }

    #[test]
    fn root_that_is_only_self_provided_yields_no_rows() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("s")
                .with_wants(vec![Behavior::build("foo")])
                .with_provides(vec![Behavior::build("foo")]),
        );

        let resolution = tracker.resolve("foo");
        assert_eq!(
            outline(&contract_rows("s", &resolution, true)),
            "s --> foo\n    OPTION: s\n"
        );
        assert!(contract_rows("s", &resolution, false).is_empty());
    }

    #[test]
    fn root_keeps_its_heading_when_a_hidden_offer_has_downstream() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("s")
                .with_wants(vec![Behavior::build("x")])
                .with_provides(vec![behavior_with("x", &["y"])]),
        );
        tracker.add_agent(Agent::build("helper").with_provides(vec![Behavior::build("y")]));

        let rows = contract_rows("s", &tracker.resolve("x"), false);

        assert_eq!(
            outline(&rows),
            "s --> x\n\
             \x20   y\n\
             \x20       OPTION: helper\n"
        );
    }

    #[test]
    fn other_providers_are_untouched() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("s")
                .with_wants(vec![Behavior::build("x")])
                .with_provides(vec![Behavior::build("x")]),
        );
        tracker.add_agent(Agent::build("other").with_provides(vec![Behavior::build("x")]));

        let rows = contract_rows("s", &tracker.resolve("x"), false);

        assert_eq!(outline(&rows), "s --> x\n    OPTION: other\n");
    }

    #[test]
    fn unsatisfied_and_error_rows_are_unaffected() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("b").with_wants(vec![Behavior::build("pa")]));
        tracker.add_agent(Agent::build("a").with_provides(vec![behavior_with("pa", &["nope"])]));

        for show_self in [true, false] {
            let rows = contract_rows("b", &tracker.resolve("pa"), show_self);
            assert_eq!(
                outline(&rows),
                "b --> pa\n\
                 \x20   POSSIBLE: a\n\
                 \x20       nope UNSATISFIED\n",
                "show_self={}",
                show_self
            );
        }
    }
}
