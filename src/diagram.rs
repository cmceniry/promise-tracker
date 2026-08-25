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
}

impl Default for DiagramOptions {
    /// Everything is drawn. Written out rather than derived, since the derived
    /// `false` would be the opposite of the long-standing behavior.
    fn default() -> Self {
        DiagramOptions {
            show_self_promises: true,
        }
    }
}

/// The lines for one offer: its arrow, unless hidden, then its conditions.
fn offer_lines(
    component: &str,
    behavior: &str,
    offer: &Offer,
    options: DiagramOptions,
) -> Vec<String> {
    let agent_name = offer.get_agent_name();
    let hidden = !options.show_self_promises && component == agent_name;

    let mut ret = Vec::new();
    if !hidden {
        ret.push(format!(
            "    {} ->> {}: {}",
            component, agent_name, behavior
        ));
    }

    // Recursively process nested conditions
    for condition in offer.get_resolved_conditions() {
        let child_lines = generate_lines(
            DiagramInput {
                component: agent_name,
                behavior: condition.get_behavior_name(),
                satisfied: condition.get_satisfying_offers(),
                unsatisfied: condition.get_unsatisfying_offers(),
            },
            options,
        );
        // Indent child lines
        for line in child_lines {
            ret.push(format!("    {}", line));
        }
    }

    ret
}

/// Recursively generate diagram lines for a resolution.
///
/// Returns a vector of diagram lines (without the leading indentation for the sequenceDiagram block).
/// Whether every offer in a group comes from the agent that needs it.
fn only_self_offers(component: &str, offers: &[Offer], options: DiagramOptions) -> bool {
    !options.show_self_promises
        && offers
            .iter()
            .all(|offer| offer.get_agent_name() == component)
}

/// One group of offers: their lines inside a colored rect.
///
/// The body is built first, so a rect is never opened around nothing. A group
/// whose offers were all hidden loses its rect too, rather than drawing an
/// empty box where the hidden promise used to be; whatever those offers needed
/// from other agents is still carried out, in its own rects.
fn group_lines(
    component: &str,
    behavior: &str,
    offers: &[Offer],
    color: &str,
    options: DiagramOptions,
) -> Vec<String> {
    let body: Vec<String> = offers
        .iter()
        .flat_map(|offer| offer_lines(component, behavior, offer, options))
        .collect();

    if body.is_empty() || only_self_offers(component, offers, options) {
        return body;
    }

    let mut ret = vec![format!("rect {}", color)];
    ret.extend(body);
    ret.push("end".to_string());
    ret
}

fn generate_lines(input: DiagramInput, options: DiagramOptions) -> Vec<String> {
    let DiagramInput {
        component,
        behavior,
        satisfied,
        unsatisfied,
    } = input;

    let mut ret = Vec::new();

    // Handle satisfied offers (green rectangle)
    ret.extend(group_lines(
        component,
        behavior,
        satisfied,
        "rgb(0,255,0)",
        options,
    ));

    // Handle unsatisfied offers (red rectangle)
    ret.extend(group_lines(
        component,
        behavior,
        unsatisfied,
        "rgb(255,0,0)",
        options,
    ));

    // Handle case with no offers (error state - red rectangle with X).
    // Keyed off the original offers, not the rendered body: "nobody provides
    // this" is a different statement from "the only offer was hidden", and
    // this line's matching names are notation rather than a self promise.
    if satisfied.is_empty() && unsatisfied.is_empty() {
        ret.push("rect rgb(255,0,0)".to_string());
        ret.push(format!("    {} -X {}: {}", component, component, behavior));
        ret.push("end".to_string());
    }

    ret
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
    let lines = generate_lines(
        DiagramInput {
            component,
            behavior,
            satisfied: resolution.get_satisfying_offers(),
            unsatisfied: resolution.get_unsatisfying_offers(),
        },
        options,
    );

    // Build the final diagram with proper indentation
    let mut result = vec!["sequenceDiagram".to_string()];
    if lines.is_empty() {
        // Only reachable with self promises hidden: every offer came from the
        // requirer itself and none of them had conditions. A bare
        // "sequenceDiagram" renders as a blank with no explanation.
        result.push(format!(
            "    Note over {}: {} is self-provided and hidden",
            component, behavior
        ));
    } else {
        for line in lines {
            result.push(format!("    {}", line));
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
}
