use leptos::prelude::*;
use promise_tracker::resolve::{Offer, Resolution};
use promise_tracker::Tracker;

use super::display_options::ViewOptions;
use super::view_frame::ViewFrame;
use crate::utils::{escape_html, generated_at, html_document, slugify};

/// One line of the resolution tree, with whatever hangs under it.
///
/// The shape is worked out as plain data first so that what the view shows can
/// be reasoned about and tested without a browser.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    class: &'static str,
    label: String,
    children: Vec<Row>,
    /// The provider, on a row that stands for an offer.
    agent: Option<String>,
}

impl Row {
    fn new(class: &'static str, label: String, children: Vec<Row>) -> Row {
        Row {
            class,
            label,
            children,
            agent: None,
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
fn offer_rows(
    requirer: &str,
    offer: &Offer,
    is_satisfied: bool,
    show_self: bool,
    mark: bool,
) -> Vec<Row> {
    let agent_name = offer.get_agent_name().to_string();
    let conditions = offer.get_resolved_conditions();

    if !show_self && agent_name == requirer {
        return conditions
            .iter()
            .flat_map(|c| resolution_rows(&agent_name, c, show_self, mark))
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
        .flat_map(|c| resolution_rows(&agent_name, c, show_self, mark))
        .collect();

    vec![Row {
        agent: Some(agent_name),
        ..Row::new(class, label, children)
    }]
}

/// The rows for every offer, satisfying ones first.
fn option_rows(
    requirer: &str,
    satisfying: &[Offer],
    unsatisfying: &[Offer],
    show_self: bool,
    mark: bool,
) -> Vec<Row> {
    let mut options: Vec<Row> = satisfying
        .iter()
        .flat_map(|offer| offer_rows(requirer, offer, true, show_self, mark))
        .collect();

    options.extend(
        unsatisfying
            .iter()
            .flat_map(|offer| offer_rows(requirer, offer, false, show_self, mark)),
    );

    options
}

/// The rows for a behavior and whoever offers it.
///
/// Yields nothing when the behavior was only ever self-provided; whatever that
/// hidden provider needed from other agents rises to the caller's list.
///
/// With `mark`, the name is shown with every substituted value in `<>`: what
/// the requirer filled in, and what a provider names through a variable.
fn resolution_rows(
    requirer: &str,
    resolution: &Resolution,
    show_self: bool,
    mark: bool,
) -> Vec<Row> {
    let behavior_name = if mark {
        resolution.marked_name_with_offers()
    } else {
        resolution.get_behavior_name().to_string()
    };
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return vec![Row::leaf(
            "contract-text-possible",
            format!("{} UNSATISFIED", behavior_name),
        )];
    }

    let options = option_rows(requirer, satisfying, unsatisfying, show_self, mark);

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
#[cfg(test)]
fn contract_rows(component: &str, resolution: &Resolution, show_self: bool) -> Vec<Row> {
    contract_rows_with(component, resolution, show_self, false)
}

/// [`contract_rows`], optionally with substituted values marked. The heading
/// shows the want as the component wrote it, so the resolution should come
/// from `Tracker::resolve_want`.
fn contract_rows_with(
    component: &str,
    resolution: &Resolution,
    show_self: bool,
    mark: bool,
) -> Vec<Row> {
    let behavior_name = if mark {
        resolution.marked_name()
    } else {
        resolution.get_behavior_name().to_string()
    };
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return vec![Row::leaf(
            "contract-text-possible",
            format!("{} --> {} UNSATISFIED", component, behavior_name),
        )];
    }

    let options = option_rows(component, satisfying, unsatisfying, show_self, mark);

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

/// A behavior row with a single offer under it: the provider, the
/// behavior, whether it is held, and what the offer rests on.
fn sole_provider(row: &Row) -> Option<(&str, &str, bool, &[Row])> {
    if row.agent.is_some() {
        return None;
    }
    match row.children.as_slice() {
        [offer] => offer.agent.as_deref().map(|agent| {
            (
                agent,
                row.label.as_str(),
                offer.class == "contract-text-option",
                offer.children.as_slice(),
            )
        }),
        _ => None,
    }
}

/// The tree with sibling needs met by the same provider grouped into one
/// row per provider, at every level.
///
/// Only a need with a single offer is grouped: a need with alternatives
/// keeps its own row, so the choice between providers stays readable. The
/// grouped row is held only when every need in it is, names the ones that
/// are not, and carries everything the grouped offers rest on.
fn bundle_rows(rows: Vec<Row>) -> Vec<Row> {
    let rows: Vec<Row> = rows
        .into_iter()
        .map(|row| Row {
            children: bundle_rows(row.children),
            ..row
        })
        .collect();

    // Providers with more than one distinct sole-provider need in this list.
    // The tree repeats a shared condition under each need that rests on it,
    // so the same behavior can come up twice; it is still one promise.
    let mut seen: Vec<(&str, &str)> = vec![];
    for row in &rows {
        if let Some((agent, behavior, ..)) = sole_provider(row) {
            if !seen.contains(&(agent, behavior)) {
                seen.push((agent, behavior));
            }
        }
    }
    let counts: Vec<(String, usize)> =
        seen.iter()
            .fold(Vec::<(String, usize)>::new(), |mut acc, (agent, _)| {
                match acc.iter_mut().find(|(a, _)| a == agent) {
                    Some((_, n)) => *n += 1,
                    None => acc.push((agent.to_string(), 1)),
                }
                acc
            });
    if counts.iter().all(|(_, n)| *n < 2) {
        return rows;
    }

    /// One provider's needs, held in place of the first one grouped.
    struct Group {
        agent: String,
        needs: Vec<(String, bool)>,
        rests_on: Vec<Row>,
    }

    let mut out: Vec<Result<Row, Group>> = vec![];
    for row in rows {
        let grouped = sole_provider(&row)
            .filter(|(agent, ..)| counts.iter().any(|(a, n)| a == agent && *n > 1))
            .map(|(agent, behavior, held, rests_on)| {
                (
                    agent.to_string(),
                    behavior.to_string(),
                    held,
                    rests_on.to_vec(),
                )
            });
        let Some((agent, behavior, held, rests_on)) = grouped else {
            out.push(Ok(row));
            continue;
        };
        let existing = out.iter_mut().find_map(|r| match r {
            Err(g) if g.agent == agent => Some(g),
            _ => None,
        });
        match existing {
            Some(g) => match g.needs.iter_mut().find(|(b, _)| *b == behavior) {
                // The same promise reached again: held only if it always is,
                // and what it rests on is already there.
                Some((_, h)) => *h &= held,
                None => {
                    g.needs.push((behavior, held));
                    // A condition two grouped needs share is shown once.
                    for row in rests_on {
                        if !g.rests_on.contains(&row) {
                            g.rests_on.push(row);
                        }
                    }
                }
            },
            None => out.push(Err(Group {
                agent,
                needs: vec![(behavior, held)],
                rests_on,
            })),
        }
    }

    out.into_iter()
        .map(|r| match r {
            Ok(row) => row,
            Err(Group {
                agent,
                needs,
                rests_on,
            }) => {
                let held = needs.iter().all(|(_, h)| *h);
                let names = needs
                    .iter()
                    .map(|(b, h)| {
                        if *h {
                            b.clone()
                        } else {
                            format!("{} \u{2717}", b)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                Row {
                    class: if held {
                        "contract-text-option"
                    } else {
                        "contract-text-possible"
                    },
                    label: format!(
                        "{}: {} \u{2014} {} \u{00d7}{}",
                        if held { "OPTION" } else { "POSSIBLE" },
                        agent,
                        names,
                        needs.len()
                    ),
                    // What the grouped offers rest on may group again.
                    children: bundle_rows(rests_on),
                    agent: Some(agent),
                }
            }
        })
        .collect()
}

/// The rows for the view: the resolution tree, with the view options applied.
fn view_rows(component: &str, resolution: &Resolution, options: ViewOptions) -> Vec<Row> {
    let rows = contract_rows_with(
        component,
        resolution,
        !options.fold_self,
        options.mark_bound,
    );
    if options.bundle {
        bundle_rows(rows)
    } else {
        rows
    }
}

/// The same tree `render_row` builds, as standalone markup.
///
/// Kept beside `render_row`: a case added to one belongs in the other. Labels
/// are escaped here because nothing else will - the live view gets that from
/// Leptos, a string built by hand does not.
fn rows_to_html(rows: &[Row]) -> String {
    fn walk(rows: &[Row], out: &mut String) {
        out.push_str("<ul class=\"contract-text-list\">");
        for row in rows {
            out.push_str("<li class=\"");
            out.push_str(row.class);
            out.push_str("\">");
            out.push_str(&escape_html(&row.label));
            if !row.children.is_empty() {
                walk(&row.children, out);
            }
            out.push_str("</li>");
        }
        out.push_str("</ul>");
    }

    let mut out = String::new();
    walk(rows, &mut out);
    out
}

/// A standalone page for one resolution tree. No scripts, no network.
fn text_export_html(title: &str, subtitle: &str, rows: &[Row]) -> String {
    let body = format!(
        "<div class=\"card\"><div class=\"card-body contract-text-card\">{}</div></div>",
        rows_to_html(rows)
    );
    html_document(title, subtitle, "", &body)
}

/// Renders one row and its children as nested list items.
fn render_row(row: Row) -> AnyView {
    let Row {
        class,
        label,
        children,
        ..
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
    /// The display options shared by every view
    #[prop(into)]
    options: Signal<ViewOptions>,
    /// Which simulation this panel belongs to, for the export's header
    sim_label: String,
) -> impl IntoView {
    let sim_for_title = sim_label.clone();
    let sim_for_file = sim_label.clone();

    // The rows the export needs are the rows the view is showing, so both go
    // through `contract_rows` with the same arguments.
    let export_rows = move || -> Option<Vec<Row>> {
        let pt = tracker.get_untracked()?;
        let component = selected_component.get_untracked();
        let behavior = selected_behavior.get_untracked();
        if component == "---" || behavior == "---" || !pt.has_ground_behavior(&behavior) {
            return None;
        }
        let rows = view_rows(
            &component,
            &pt.resolve_want(&component, &behavior),
            options.get_untracked(),
        );
        if rows.is_empty() {
            None
        } else {
            Some(rows)
        }
    };

    let frame_title = Signal::derive(move || format!("Text View - Simulation {}", sim_for_title));
    let frame_subtitle = Signal::derive(move || {
        let focus = format!(
            "{} -> {}",
            selected_component.get(),
            selected_behavior.get()
        );
        format!(
            "{} - {} - generated {}",
            focus,
            options.get().describe(),
            generated_at()
        )
    });
    let frame_filename = Signal::derive(move || {
        format!(
            "promise-tracker-text-{}-{}-{}",
            slugify(&sim_for_file),
            slugify(&selected_component.get()),
            slugify(&selected_behavior.get())
        )
    });
    let export = Callback::new(move |_: ()| {
        export_rows().map(|rows| {
            text_export_html(
                &frame_title.get_untracked(),
                &frame_subtitle.get_untracked(),
                &rows,
            )
        })
    });
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

        if !pt.has_ground_behavior(&behavior) {
            return view! {
                <div class="text-muted p-3">"Select a valid behavior"</div>
            }
            .into_any();
        }

        // Resolve the behavior and render the result
        // Resolved as the component wants it, so that marking can show
        // what its own declaration filled in.
        let resolution = pt.resolve_want(&component, &behavior);
        let rows = view_rows(&component, &resolution, options.get());

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

    view! {
        <ViewFrame
            title=frame_title
            subtitle=frame_subtitle
            filename=frame_filename
            export=export
        >
            <div class="contract-text">{content}</div>
        </ViewFrame>
    }
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
    fn export_markup_matches_the_row_tree() {
        let tracker = self_condition_tracker();
        let rows = contract_rows("b", &tracker.resolve("pa"), false);

        assert_eq!(
            rows_to_html(&rows),
            concat!(
                "<ul class=\"contract-text-list\">",
                "<li class=\"contract-text-option\">b --&gt; pa",
                "<ul class=\"contract-text-list\">",
                "<li class=\"contract-text-option\">OPTION: a",
                "<ul class=\"contract-text-list\">",
                "<li class=\"contract-text-option\">pc",
                "<ul class=\"contract-text-list\">",
                "<li class=\"contract-text-option\">OPTION: c</li>",
                "</ul></li></ul></li></ul></li></ul>",
            )
        );
    }

    #[test]
    fn export_escapes_names_from_the_contract() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("b").with_wants(vec![Behavior::build("pa")]));
        tracker.add_agent(Agent::build("a<b> & \"c\"").with_provides(vec![Behavior::build("pa")]));

        let rows = contract_rows("b", &tracker.resolve("pa"), true);
        let html = rows_to_html(&rows);

        assert!(
            html.contains("OPTION: a&lt;b&gt; &amp; &quot;c&quot;"),
            "{}",
            html
        );
        assert!(!html.contains("<b>"), "{}", html);
    }

    #[test]
    fn export_page_is_offline_and_points_nowhere() {
        let tracker = self_condition_tracker();
        let rows = contract_rows("b", &tracker.resolve("pa"), true);
        let page = text_export_html("Text View - Simulation A", "b -> pa", &rows);

        assert!(page.starts_with("<!DOCTYPE html>"));
        assert!(page.contains("OPTION: a"));
        // Nothing that needs the network, and no route back to the app
        for probe in [
            "<script",
            "<link",
            "localStorage",
            "fetch(",
            "/api/",
            "/contracts/",
            ".wasm",
            "localhost",
            "127.0.0.1",
            "pt-edge-select",
        ] {
            assert!(!page.contains(probe), "export contains {}", probe);
        }
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

    fn bundled_options() -> ViewOptions {
        ViewOptions {
            bundle: true,
            fold_self: false,
            ..ViewOptions::default()
        }
    }

    /// b wants pa from a, which rests on p1, p2 and p3 from c and on pd,
    /// which d and e both offer; p3 rests on px, which nobody offers.
    fn one_provider_tracker() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("b").with_wants(vec![Behavior::build("pa")]));
        tracker.add_agent(
            Agent::build("a").with_provides(vec![behavior_with("pa", &["p1", "p2", "pd", "p3"])]),
        );
        tracker.add_agent(Agent::build("c").with_provides(vec![
            Behavior::build("p1"),
            behavior_with("p2", &["pe"]),
            behavior_with("p3", &["px"]),
        ]));
        tracker.add_agent(Agent::build("d").with_provides(vec![Behavior::build("pd")]));
        tracker.add_agent(
            Agent::build("e").with_provides(vec![Behavior::build("pd"), Behavior::build("pe")]),
        );
        tracker
    }

    #[test]
    fn bundling_groups_sibling_needs_per_provider() {
        let tracker = one_provider_tracker();
        let rows = view_rows("b", &tracker.resolve("pa"), bundled_options());
        assert_eq!(
            outline(&rows),
            "b --> pa\n\
             \x20   POSSIBLE: a\n\
             \x20       POSSIBLE: c \u{2014} p1, p2, p3 \u{2717} \u{00d7}3\n\
             \x20           pe\n\
             \x20               OPTION: e\n\
             \x20           px UNSATISFIED\n\
             \x20       pd\n\
             \x20           OPTION: d\n\
             \x20           OPTION: e\n"
        );
        // The group is marked as not held, because p3 is not.
        assert_eq!(
            rows[0].children[0].children[0].class,
            "contract-text-possible"
        );
    }

    #[test]
    fn bundling_off_leaves_the_tree_alone() {
        let tracker = one_provider_tracker();
        let resolution = tracker.resolve("pa");
        let plain = contract_rows("b", &resolution, true);
        let off = view_rows(
            "b",
            &resolution,
            ViewOptions {
                bundle: false,
                ..bundled_options()
            },
        );
        assert_eq!(off, plain);
        // A tree with nothing to group comes out the same with it on.
        let tracker = self_condition_tracker();
        let resolution = tracker.resolve("pa");
        assert_eq!(
            view_rows("b", &resolution, bundled_options()),
            contract_rows("b", &resolution, true)
        );
    }

    /// a's pa rests on p3 and p4 from c; both rest on p5 from d. Grouped
    /// under c, p5 is one need, not two.
    #[test]
    fn bundling_counts_a_shared_condition_once() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("b").with_wants(vec![Behavior::build("pa")]));
        tracker
            .add_agent(Agent::build("a").with_provides(vec![behavior_with("pa", &["p3", "p4"])]));
        tracker.add_agent(Agent::build("c").with_provides(vec![
            behavior_with("p3", &["p5"]),
            behavior_with("p4", &["p5"]),
        ]));
        tracker.add_agent(Agent::build("d").with_provides(vec![Behavior::build("p5")]));

        let rows = view_rows("b", &tracker.resolve("pa"), bundled_options());
        assert_eq!(
            outline(&rows),
            "b --> pa\n\
             \x20   OPTION: a\n\
             \x20       OPTION: c \u{2014} p3, p4 \u{00d7}2\n\
             \x20           p5\n\
             \x20               OPTION: d\n"
        );
    }

    fn iam_tracker() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("AWSSM").with_provides(vec![behavior_with(
            "Read Secret Contents {{secret_name}} by {{application}}",
            &[
                "{{application}} has IAM role {{?approle}}",
                "IAM access granted for reading Secret {{secret_name}} to {{approle}}",
            ],
        )]));
        tracker.add_agent(Agent::build("infrateam").with_provides(vec![behavior_with(
            "IAM access granted for reading Secret {{secret_name}} to {{approle}}",
            &["Design approval access to Secret {{secret_name}} for {{?application}}"],
        )]));
        tracker.add_agent(Agent::build("manager").with_provides(vec![Behavior::build(
            "Design approval access to Secret appsecret for appname",
        )]));
        tracker.add_agent(
            Agent::build("appname")
                .with_wants(vec![Behavior::build(
                    "Read Secret Contents appsecret by {{!self}}",
                )])
                .with_provides(vec![Behavior::build("{{!self}} has IAM role approle")]),
        );
        tracker
    }

    #[test]
    fn marking_wraps_substituted_values_and_changes_nothing_else() {
        let tracker = iam_tracker();
        let want = "Read Secret Contents appsecret by appname";
        let resolution = tracker.resolve_want("appname", want);
        let off = ViewOptions {
            bundle: false,
            ..ViewOptions::default()
        };
        let on = ViewOptions {
            mark_bound: true,
            ..off
        };

        let plain = outline(&view_rows("appname", &resolution, off));
        assert!(!plain.contains('<'), "{}", plain);
        assert_eq!(
            plain,
            outline(&view_rows("appname", &tracker.resolve(want), off))
        );

        let marked = outline(&view_rows("appname", &resolution, on));
        for line in [
            "appname --> Read Secret Contents appsecret by <appname>",
            "<appname> has IAM role {{?approle}}",
            "IAM access granted for reading Secret <appsecret> to <approle>",
            "Design approval access to Secret <appsecret> for {{?application}}",
        ] {
            assert!(marked.contains(line), "no `{}` in:\n{}", line, marked);
        }
        // Take the marks away (sparing the heading's arrow) and it is the
        // same tree.
        let unmarked = marked
            .replace("-->", "\u{1}")
            .replace(['<', '>'], "")
            .replace('\u{1}', "-->");
        assert_eq!(unmarked, plain);
    }
}
