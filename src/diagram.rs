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

/// Recursively generate diagram lines for a resolution.
///
/// Returns a vector of diagram lines (without the leading indentation for the sequenceDiagram block).
fn generate_lines(input: DiagramInput) -> Vec<String> {
    let DiagramInput {
        component,
        behavior,
        satisfied,
        unsatisfied,
    } = input;

    let mut ret = Vec::new();

    // Handle satisfied offers (green rectangle)
    if !satisfied.is_empty() {
        ret.push("rect rgb(0,255,0)".to_string());
        for offer in satisfied {
            let agent_name = offer.get_agent_name();
            ret.push(format!(
                "    {} ->> {}: {}",
                component, agent_name, behavior
            ));

            // Recursively process nested conditions
            for condition in offer.get_resolved_conditions() {
                let child_lines = generate_lines(DiagramInput {
                    component: agent_name,
                    behavior: condition.get_behavior_name(),
                    satisfied: condition.get_satisfying_offers(),
                    unsatisfied: condition.get_unsatisfying_offers(),
                });
                // Indent child lines
                for line in child_lines {
                    ret.push(format!("    {}", line));
                }
            }
        }
        ret.push("end".to_string());
    }

    // Handle unsatisfied offers (red rectangle)
    if !unsatisfied.is_empty() {
        ret.push("rect rgb(255,0,0)".to_string());
        for offer in unsatisfied {
            let agent_name = offer.get_agent_name();
            ret.push(format!(
                "    {} ->> {}: {}",
                component, agent_name, behavior
            ));

            // Recursively process nested conditions
            for condition in offer.get_resolved_conditions() {
                let child_lines = generate_lines(DiagramInput {
                    component: agent_name,
                    behavior: condition.get_behavior_name(),
                    satisfied: condition.get_satisfying_offers(),
                    unsatisfied: condition.get_unsatisfying_offers(),
                });
                // Indent child lines
                for line in child_lines {
                    ret.push(format!("    {}", line));
                }
            }
        }
        ret.push("end".to_string());
    }

    // Handle case with no offers (error state - red rectangle with X)
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
    let lines = generate_lines(DiagramInput {
        component,
        behavior,
        satisfied: resolution.get_satisfying_offers(),
        unsatisfied: resolution.get_unsatisfying_offers(),
    });

    // Build the final diagram with proper indentation
    let mut result = vec!["sequenceDiagram".to_string()];
    for line in lines {
        result.push(format!("    {}", line));
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior};
    use crate::Tracker;

    #[test]
    fn test_empty_resolution() {
        let resolution = Resolution::new("b1");
        let result = diagram("c1", "b1", &resolution);

        assert!(result.contains("sequenceDiagram"));
        assert!(result.contains("rect rgb(255,0,0)"));
        assert!(result.contains("c1 -X c1: b1"));
        assert!(result.contains("end"));
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
    }
}
