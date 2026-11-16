//! Sequence diagram generation for Mermaid visualization.
//!
//! Generates Mermaid sequence diagram DSL showing promise resolution flows
//! between components and behaviors.

use crate::resolve::{Offer, Resolution};

/// Input data for generating a sequence diagram.
pub struct DiagramInput<'a> {
    pub component: &'a str,
    pub behavior: &'a str,
    pub satisfied: &'a [Offer],
    pub unsatisfied: &'a [Offer],
}

/// What to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagramOptions {
    /// When false, an offer whose provider is the requirer itself gets no
    /// arrow. Its conditions are still drawn, so whatever the hidden provider
    /// needs from *other* agents stays visible.
    pub show_self_promises: bool,
    /// When true, every promise from one participant to another becomes one
    /// arrow, labelled with all of them; see [`bundle`].
    pub bundle: bool,
}

impl Default for DiagramOptions {
    /// Everything is drawn, one arrow per promise. Written out rather than
    /// derived, since the derived `false` would be the opposite of the
    /// long-standing behavior.
    fn default() -> Self {
        DiagramOptions {
            show_self_promises: true,
            bundle: false,
        }
    }
}

/// The diagram as a tree, before it is written out as Mermaid.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    /// A group of offers. With a color it is drawn inside a rect, green when
    /// `held`; without one its body is spliced in where the rect would be.
    Group { rect: Option<bool>, body: Vec<Step> },
    /// One arrow, carrying one promise or a bundle of them.
    Arrow {
        from: String,
        to: String,
        promises: Vec<(String, bool)>,
    },
    /// Nobody provides the behavior.
    Missing { who: String, behavior: String },
}

const HELD: &str = "rgb(0,255,0)";
const BROKEN: &str = "rgb(255,0,0)";

impl Step {
    fn is_empty_group(&self) -> bool {
        matches!(self, Step::Group { body, .. } if body.is_empty())
    }

    /// Lines at this depth; a group's body is always indented one step,
    /// whether or not its rect is drawn.
    fn render(&self, indent: usize, out: &mut Vec<String>) {
        let pad = " ".repeat(indent);
        match self {
            Step::Group { rect, body } => {
                if let Some(held) = rect {
                    out.push(format!("{}rect {}", pad, if *held { HELD } else { BROKEN }));
                }
                for step in body {
                    step.render(indent + 4, out);
                }
                if rect.is_some() {
                    out.push(format!("{}end", pad));
                }
            }
            Step::Arrow { from, to, promises } => {
                out.push(format!(
                    "{}{} ->> {}: {}",
                    pad,
                    from,
                    to,
                    arrow_label(promises)
                ));
            }
            Step::Missing { who, behavior } => {
                out.push(format!("{}{} -X {}: {}", pad, who, who, behavior));
            }
        }
    }
}

/// One promise names itself; a bundle names them all, marks the broken
/// ones, and counts.
fn arrow_label(promises: &[(String, bool)]) -> String {
    match promises {
        [(behavior, _)] => behavior.clone(),
        many => format!(
            "{} \u{00d7}{}",
            many.iter()
                .map(|(b, held)| if *held {
                    b.clone()
                } else {
                    format!("{} \u{2717}", b)
                })
                .collect::<Vec<_>>()
                .join(", "),
            many.len()
        ),
    }
}

/// The steps for one offer: its arrow, unless hidden, then its conditions.
fn offer_steps(
    component: &str,
    behavior: &str,
    offer: &Offer,
    held: bool,
    options: DiagramOptions,
) -> Vec<Step> {
    let agent_name = offer.get_agent_name();
    let hidden = !options.show_self_promises && component == agent_name;

    let mut ret = Vec::new();
    if !hidden {
        ret.push(Step::Arrow {
            from: component.to_string(),
            to: agent_name.to_string(),
            promises: vec![(behavior.to_string(), held)],
        });
    }

    // Recursively process nested conditions
    for condition in offer.get_resolved_conditions() {
        ret.extend(generate_steps(
            DiagramInput {
                component: agent_name,
                behavior: condition.get_behavior_name(),
                satisfied: condition.get_satisfying_offers(),
                unsatisfied: condition.get_unsatisfying_offers(),
            },
            options,
        ));
    }

    ret
}

/// Whether every offer in a group comes from the agent that needs it.
fn only_self_offers(component: &str, offers: &[Offer], options: DiagramOptions) -> bool {
    !options.show_self_promises
        && offers
            .iter()
            .all(|offer| offer.get_agent_name() == component)
}

/// One group of offers, inside a colored rect.
///
/// A group whose offers were all hidden loses its rect, rather than drawing a
/// box where the hidden promise used to be; whatever those offers needed from
/// other agents is still carried out, in its own rects. A group with nothing
/// left in it is dropped when the diagram is written out.
fn group_step(
    component: &str,
    behavior: &str,
    offers: &[Offer],
    held: bool,
    options: DiagramOptions,
) -> Step {
    let body: Vec<Step> = offers
        .iter()
        .flat_map(|offer| offer_steps(component, behavior, offer, held, options))
        .collect();
    let rect = if only_self_offers(component, offers, options) {
        None
    } else {
        Some(held)
    };
    Step::Group { rect, body }
}

/// Recursively generate the steps for a resolution.
fn generate_steps(input: DiagramInput, options: DiagramOptions) -> Vec<Step> {
    let DiagramInput {
        component,
        behavior,
        satisfied,
        unsatisfied,
    } = input;

    let mut ret = vec![
        group_step(component, behavior, satisfied, true, options),
        group_step(component, behavior, unsatisfied, false, options),
    ];

    // Handle case with no offers (error state - red rectangle with X).
    // Keyed off the original offers, not the rendered body: "nobody provides
    // this" is a different statement from "the only offer was hidden", and
    // this line's matching names are notation rather than a self promise.
    if satisfied.is_empty() && unsatisfied.is_empty() {
        ret.push(Step::Group {
            rect: Some(false),
            body: vec![Step::Missing {
                who: component.to_string(),
                behavior: behavior.to_string(),
            }],
        });
    }

    ret
}

/// Drop groups left with nothing inside, innermost first, so a rect is never
/// opened around nothing.
fn prune(steps: &mut Vec<Step>) {
    for step in steps.iter_mut() {
        if let Step::Group { body, .. } = step {
            prune(body);
        }
    }
    steps.retain(|s| !s.is_empty_group());
}

/// Every promise from one participant to another, merged into the first
/// arrow between them, each distinct behavior once.
///
/// The later arrows go, and whatever their conditions led to stays where it
/// was. A bundle with a broken promise inside never sits in a green rect
/// looking held: it gets a red rect of its own there.
fn bundle(steps: &mut Vec<Step>) {
    /// A pair of participants, and every promise between them in order.
    type Pair = ((String, String), Vec<(String, bool)>);

    fn collect(steps: &[Step], into: &mut Vec<Pair>) {
        for step in steps {
            match step {
                Step::Group { body, .. } => collect(body, into),
                Step::Arrow { from, to, promises } => {
                    let key = (from.clone(), to.clone());
                    let at = match into.iter().position(|(k, _)| *k == key) {
                        Some(at) => at,
                        None => {
                            into.push((key, vec![]));
                            into.len() - 1
                        }
                    };
                    // The tree repeats a shared condition under each step
                    // that needs it; it is still one promise, held only if
                    // every time it is reached it is.
                    let all = &mut into[at].1;
                    for (behavior, held) in promises {
                        match all.iter_mut().find(|(b, _)| b == behavior) {
                            Some((_, h)) => *h &= *held,
                            None => all.push((behavior.clone(), *held)),
                        }
                    }
                }
                Step::Missing { .. } => {}
            }
        }
    }

    fn rewrite(
        steps: &mut Vec<Step>,
        in_held_rect: bool,
        bundles: &[Pair],
        placed: &mut Vec<(String, String)>,
    ) {
        let old = std::mem::take(steps);
        for step in old {
            match step {
                Step::Group { rect, mut body } => {
                    let held = rect.unwrap_or(in_held_rect);
                    rewrite(&mut body, held, bundles, placed);
                    steps.push(Step::Group { rect, body });
                }
                Step::Arrow { from, to, .. } => {
                    let key = (from.clone(), to.clone());
                    if placed.contains(&key) {
                        continue;
                    }
                    let promises = bundles
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, p)| p.clone())
                        .unwrap_or_default();
                    let broken = promises.iter().any(|(_, held)| !held);
                    placed.push(key);
                    let arrow = Step::Arrow { from, to, promises };
                    if broken && in_held_rect {
                        steps.push(Step::Group {
                            rect: Some(false),
                            body: vec![arrow],
                        });
                    } else {
                        steps.push(arrow);
                    }
                }
                missing => steps.push(missing),
            }
        }
    }

    let mut bundles = vec![];
    collect(steps, &mut bundles);
    rewrite(steps, false, &bundles, &mut vec![]);
}

/// Generate a Mermaid sequence diagram DSL string from resolution data.
///
/// # Arguments
/// * `component` - The component (agent) requesting the behavior
/// * `behavior` - The behavior being resolved
/// * `resolution` - The resolution data from the tracker
///
/// # Returns
/// A string containing the complete Mermaid sequence diagram DSL.
pub fn diagram(component: &str, behavior: &str, resolution: &Resolution) -> String {
    diagram_with(component, behavior, resolution, DiagramOptions::default())
}

/// Generate a Mermaid sequence diagram DSL string, choosing what to draw.
///
/// See [`diagram`] for the default rendering.
pub fn diagram_with(
    component: &str,
    behavior: &str,
    resolution: &Resolution,
    options: DiagramOptions,
) -> String {
    let mut steps = generate_steps(
        DiagramInput {
            component,
            behavior,
            satisfied: resolution.get_satisfying_offers(),
            unsatisfied: resolution.get_unsatisfying_offers(),
        },
        options,
    );
    if options.bundle {
        bundle(&mut steps);
    }
    prune(&mut steps);

    // Build the final diagram with proper indentation
    let mut result = vec!["sequenceDiagram".to_string()];
    if steps.is_empty() {
        // Only reachable with self promises hidden: every offer came from the
        // requirer itself and none of them had conditions. A bare
        // "sequenceDiagram" renders as a blank with no explanation.
        result.push(format!(
            "    Note over {}: {} is self-provided and hidden",
            component, behavior
        ));
    } else {
        for step in &steps {
            step.render(4, &mut result);
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior};
    use crate::Tracker;

    /// Mermaid will not render a rect that was opened around nothing.
    fn assert_no_empty_rect(dsl: &str) {
        let lines: Vec<&str> = dsl.lines().map(str::trim).collect();
        for pair in lines.windows(2) {
            assert!(
                !(pair[0].starts_with("rect ") && pair[1] == "end"),
                "empty rect in:\n{}",
                dsl
            );
        }
    }

    fn hide_self() -> DiagramOptions {
        DiagramOptions {
            show_self_promises: false,
            ..DiagramOptions::default()
        }
    }

    #[test]
    fn test_empty_resolution() {
        let resolution = Resolution::new("b1");
        let result = diagram("c1", "b1", &resolution);

        assert!(result.contains("sequenceDiagram"));
        assert!(result.contains("rect rgb(255,0,0)"));
        assert!(result.contains("c1 -X c1: b1"));
        assert!(result.contains("end"));
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_satisfied_resolution() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));

        let resolution = tracker.resolve("b1");
        let result = diagram("c1", "b1", &resolution);

        assert!(result.contains("sequenceDiagram"));
        assert!(result.contains("rect rgb(0,255,0)"));
        assert!(result.contains("c1 ->> a1: b1"));
        assert!(result.contains("end"));
        // Should not contain red rectangle or X
        assert!(!result.contains("rgb(255,0,0)"));
        assert!(!result.contains("-X"));
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_unsatisfied_resolution() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));

        let resolution = tracker.resolve("b1");
        let result = diagram("c1", "b1", &resolution);

        assert!(result.contains("sequenceDiagram"));
        // Should have red rectangle for unsatisfied
        assert!(result.contains("rect rgb(255,0,0)"));
        assert!(result.contains("c1 ->> a1: b1"));
        // Nested unmet condition should show as error
        assert!(result.contains("a1 -X a1: b2"));
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_mixed_resolution() {
        let mut tracker = Tracker::new();
        // a1 provides b1 unconditionally
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        // a2 provides b1 with condition b2 (unsatisfied)
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));

        let resolution = tracker.resolve("b1");
        let result = diagram("c1", "b1", &resolution);

        // Should have both green and red sections
        assert!(result.contains("rect rgb(0,255,0)"));
        assert!(result.contains("rect rgb(255,0,0)"));
        assert!(result.contains("c1 ->> a1: b1")); // satisfied
        assert!(result.contains("c1 ->> a2: b1")); // unsatisfied path
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_nested_satisfied_conditions() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));

        let resolution = tracker.resolve("b1");
        let result = diagram("c1", "b1", &resolution);

        // All satisfied, should only have green
        assert!(result.contains("rect rgb(0,255,0)"));
        assert!(result.contains("c1 ->> a1: b1"));
        assert!(result.contains("a1 ->> a2: b2")); // nested condition
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_default_options_match_the_plain_diagram() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));

        let resolution = tracker.resolve("b1");

        // Exact output, so the restructuring cannot drift from what shipped.
        assert_eq!(
            diagram("c1", "b1", &resolution),
            concat!(
                "sequenceDiagram\n",
                "    rect rgb(0,255,0)\n",
                "        c1 ->> a1: b1\n",
                "        rect rgb(0,255,0)\n",
                "            a1 ->> a2: b2\n",
                "        end\n",
                "    end",
            )
        );
        assert_eq!(
            diagram("c1", "b1", &resolution),
            diagram_with("c1", "b1", &resolution, DiagramOptions::default())
        );
    }

    #[test]
    fn test_hide_self_drops_the_arrow_and_keeps_conditions() {
        let mut tracker = Tracker::new();
        // a1 provides b1 for itself, but only if a2 provides b2.
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));

        let resolution = tracker.resolve("b1");
        let result = diagram_with("a1", "b1", &resolution, hide_self());

        assert!(!result.contains("a1 ->> a1: b1"), "{}", result);
        assert!(result.contains("a1 ->> a2: b2"), "{}", result);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_hide_self_with_no_conditions_leaves_a_note() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));

        let resolution = tracker.resolve("b1");
        let result = diagram_with("a1", "b1", &resolution, hide_self());

        assert_eq!(
            result,
            "sequenceDiagram\n    Note over a1: b1 is self-provided and hidden"
        );
        assert!(!result.contains("rect"));
        assert!(!result.contains("->>"));
    }

    #[test]
    fn test_hide_self_keeps_the_unresolvable_marker() {
        // The -X line names the same participant twice, but it means "nobody
        // provides this", not "an agent provides it to itself".
        let resolution = Resolution::new("b1");
        let result = diagram_with("c1", "b1", &resolution, hide_self());

        assert!(result.contains("rect rgb(255,0,0)"), "{}", result);
        assert!(result.contains("c1 -X c1: b1"), "{}", result);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_hide_self_keeps_other_providers() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let resolution = tracker.resolve("b1");
        let result = diagram_with("a1", "b1", &resolution, hide_self());

        assert!(!result.contains("a1 ->> a1: b1"), "{}", result);
        assert!(result.contains("a1 ->> a2: b1"), "{}", result);
        assert_eq!(result.matches("rect rgb(0,255,0)").count(), 1);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_hide_self_keeps_a_rect_whose_head_is_hidden() {
        let mut tracker = Tracker::new();
        // The self offer is unsatisfying, and its unmet condition still has
        // something to say, so the red rect stays open around it.
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));

        let resolution = tracker.resolve("b1");
        let result = diagram_with("a1", "b1", &resolution, hide_self());

        assert!(!result.contains("a1 ->> a1: b1"), "{}", result);
        assert!(result.contains("a1 -X a1: b2"), "{}", result);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_hide_self_drops_the_group_it_emptied() {
        let mut tracker = Tracker::new();
        // a keeps its promise to b by meeting its own condition paa, which in
        // turn needs pc from c.
        tracker.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("pa").with_conditions(vec!["paa".to_string()]),
            Behavior::build("paa").with_conditions(vec!["pc".to_string()]),
        ]));
        tracker.add_agent(Agent::build("c").with_provides(vec![Behavior::build("pc")]));

        let resolution = tracker.resolve("pa");
        let result = diagram_with("b", "pa", &resolution, hide_self());

        // No arrow for paa, and no box left standing where its group was.
        assert!(!result.contains("paa"), "{}", result);
        assert!(result.contains("b ->> a: pa"), "{}", result);
        assert!(result.contains("a ->> c: pc"), "{}", result);
        assert_eq!(result.matches("rect ").count(), 2);
        assert_eq!(result.matches("end").count(), 2);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_hide_self_on_a_nested_condition() {
        let mut tracker = Tracker::new();
        // a1 keeps its promise to c1 by meeting its own condition.
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
            Behavior::build("b2"),
        ]));

        let resolution = tracker.resolve("b1");
        let result = diagram_with("c1", "b1", &resolution, hide_self());

        assert!(result.contains("c1 ->> a1: b1"), "{}", result);
        assert!(!result.contains("a1 ->> a1: b2"), "{}", result);
        assert_no_empty_rect(&result);
    }

    fn bundled() -> DiagramOptions {
        DiagramOptions {
            bundle: true,
            ..DiagramOptions::default()
        }
    }

    /// a1 keeps b1 on b2 and b3, both from a2; b3 rests on b4 from a3.
    fn two_promises_one_pair() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string(), "b3".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b2"),
            Behavior::build("b3").with_conditions(vec!["b4".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b4")]));
        tracker
    }

    #[test]
    fn test_bundle_merges_arrows_between_one_pair() {
        let resolution = two_promises_one_pair().resolve("b1");
        let plain = diagram("c1", "b1", &resolution);
        assert!(plain.contains("a1 ->> a2: b2"), "{}", plain);
        assert!(plain.contains("a1 ->> a2: b3"), "{}", plain);

        let result = diagram_with("c1", "b1", &resolution, bundled());
        assert_eq!(
            result,
            concat!(
                "sequenceDiagram\n",
                "    rect rgb(0,255,0)\n",
                "        c1 ->> a1: b1\n",
                "        rect rgb(0,255,0)\n",
                "            a1 ->> a2: b2, b3 \u{00d7}2\n",
                "        end\n",
                "        rect rgb(0,255,0)\n",
                "            rect rgb(0,255,0)\n",
                "                a2 ->> a3: b4\n",
                "            end\n",
                "        end\n",
                "    end",
            )
        );
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_bundle_with_nothing_to_merge_matches_the_plain_diagram() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        let resolution = tracker.resolve("b1");
        assert_eq!(
            diagram_with("c1", "b1", &resolution, bundled()),
            diagram("c1", "b1", &resolution)
        );
    }

    #[test]
    fn test_bundle_never_shows_a_broken_promise_as_held() {
        // a1 keeps b1 on b2 and b3 from a2, but b3 rests on b4 which nobody
        // promises: the a1 -> a2 bundle is green for b2 and red for b3.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string(), "b3".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b2"),
            Behavior::build("b3").with_conditions(vec!["b4".to_string()]),
        ]));
        let resolution = tracker.resolve("b1");
        let result = diagram_with("c1", "b1", &resolution, bundled());

        assert!(
            result.contains("a1 ->> a2: b2, b3 \u{2717} \u{00d7}2"),
            "{}",
            result
        );
        assert_eq!(result.matches("->> a2").count(), 1, "{}", result);
        // The bundle sits where b2's green rect was, so it gets a red one.
        let lines: Vec<&str> = result.lines().map(str::trim).collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("a1 ->> a2"))
            .unwrap();
        assert_eq!(lines[at - 1], "rect rgb(255,0,0)", "{}", result);
        // The unmet condition is still drawn.
        assert!(result.contains("a2 -X a2: b4"), "{}", result);
        assert_no_empty_rect(&result);
    }

    #[test]
    fn test_bundle_keeps_directions_apart() {
        // c1 -> a1 for b1, a1 -> c1 for b2: two pairs, not one.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("c1").with_provides(vec![Behavior::build("b2")]));
        let resolution = tracker.resolve("b1");
        let result = diagram_with("c1", "b1", &resolution, bundled());
        assert!(result.contains("c1 ->> a1: b1"), "{}", result);
        assert!(result.contains("a1 ->> c1: b2"), "{}", result);
    }

    /// a1's b1 rests on b3 and b4 from a2; both rest on b5 from a3. The tree
    /// reaches a2 -> a3: b5 twice, but it is one promise.
    #[test]
    fn test_bundle_counts_a_shared_condition_once() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b3".to_string(), "b4".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b3").with_conditions(vec!["b5".to_string()]),
            Behavior::build("b4").with_conditions(vec!["b5".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b5")]));
        let resolution = tracker.resolve("b1");
        let result = diagram_with("c1", "b1", &resolution, bundled());

        assert!(result.contains("a1 ->> a2: b3, b4 \u{00d7}2"), "{}", result);
        assert!(result.contains("a2 ->> a3: b5\n"), "{}", result);
        assert!(!result.contains("b5, b5"), "{}", result);
        assert_eq!(result.matches("->> a3").count(), 1, "{}", result);
        assert_no_empty_rect(&result);
    }
}
