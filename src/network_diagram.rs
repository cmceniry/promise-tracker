//! Network diagram generation for force-directed graph visualization.
//!
//! Generates graph data (nodes and links) showing promise relationships
//! between components and behaviors.

use crate::resolve::{Offer, Resolution};
use crate::Tracker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of node in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Component,
    Behavior,
}

/// Type of relationship between nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    Wants,
    Provides,
    Needs,
}

/// A node in the force-directed graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub satisfied: bool,
}

/// A link between nodes in the graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub link_type: LinkType,
    pub satisfied: bool,
}

/// The complete graph data structure for rendering
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

impl GraphData {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Builder for constructing graph data from a tracker
struct GraphBuilder {
    nodes: Vec<GraphNode>,
    links: Vec<GraphLink>,
    node_map: HashMap<String, usize>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
            node_map: HashMap::new(),
        }
    }

    /// Get or create a node, returning its index
    fn get_or_create_node(&mut self, id: &str, node_type: NodeType) -> usize {
        if let Some(&idx) = self.node_map.get(id) {
            return idx;
        }

        let idx = self.nodes.len();
        self.nodes.push(GraphNode {
            id: id.to_string(),
            label: id.to_string(),
            node_type,
            satisfied: true, // default, will be updated if unsatisfied
        });
        self.node_map.insert(id.to_string(), idx);
        idx
    }

    /// Check if a link already exists
    fn link_exists(&self, source: &str, target: &str, link_type: LinkType) -> bool {
        self.links.iter().any(|link| {
            link.source == source && link.target == target && link.link_type == link_type
        })
    }

    /// Find a link if it exists and return a mutable reference to its index
    fn find_link(&self, source: &str, target: &str, link_type: LinkType) -> Option<usize> {
        self.links.iter().position(|link| {
            link.source == source && link.target == target && link.link_type == link_type
        })
    }

    /// Process a resolution recursively to extract all nested relationships
    fn process_resolution(&mut self, behavior_name: &str, resolution: &Resolution) {
        let satisfied = resolution.get_satisfying_offers();
        let unsatisfied = resolution.get_unsatisfying_offers();

        // Process satisfied offers
        for offer in satisfied {
            self.process_offer(behavior_name, offer, true);
        }

        // Process unsatisfied offers
        for offer in unsatisfied {
            self.process_offer(behavior_name, offer, false);
        }

        // Update behavior node satisfaction status
        if let Some(&idx) = self.node_map.get(behavior_name) {
            if satisfied.is_empty() && unsatisfied.is_empty() {
                self.nodes[idx].satisfied = false;
            } else if !unsatisfied.is_empty() && satisfied.is_empty() {
                self.nodes[idx].satisfied = false;
            }
        }
    }

    /// Process an offer (satisfied or unsatisfied)
    fn process_offer(&mut self, behavior_name: &str, offer: &Offer, is_satisfied: bool) {
        let provider_name = offer.get_agent_name();

        // Ensure provider component node exists
        self.get_or_create_node(provider_name, NodeType::Component);

        // Create link from behavior to provider (provides relationship)
        if !self.link_exists(behavior_name, provider_name, LinkType::Provides) {
            self.links.push(GraphLink {
                source: behavior_name.to_string(),
                target: provider_name.to_string(),
                link_type: LinkType::Provides,
                satisfied: is_satisfied,
            });
        }

        // Process nested conditions
        for condition in offer.get_resolved_conditions() {
            let condition_behavior_name = condition.get_behavior_name();

            // Ensure condition behavior node exists
            self.get_or_create_node(condition_behavior_name, NodeType::Behavior);

            // Determine if this condition is satisfied
            let condition_satisfied = !condition.get_satisfying_offers().is_empty();

            // Create link from provider component to condition behavior (needs condition)
            if !self.link_exists(provider_name, condition_behavior_name, LinkType::Needs) {
                self.links.push(GraphLink {
                    source: provider_name.to_string(),
                    target: condition_behavior_name.to_string(),
                    link_type: LinkType::Needs,
                    satisfied: condition_satisfied,
                });
            } else {
                // Update existing link's satisfied status if needed
                if let Some(idx) =
                    self.find_link(provider_name, condition_behavior_name, LinkType::Needs)
                {
                    self.links[idx].satisfied = condition_satisfied;
                }
            }

            // Recursively process the condition's resolution
            self.process_resolution(condition_behavior_name, condition);
        }
    }

    /// Build the final graph data
    fn build(self) -> GraphData {
        GraphData {
            nodes: self.nodes,
            links: self.links,
        }
    }
}

/// Generate network graph data from a Tracker
///
/// This function analyzes all agents in the tracker and builds a graph
/// showing:
/// - Components (agents) as blue nodes
/// - Behaviors as green (satisfied) or red (unsatisfied) nodes
/// - Links showing wants/provides/needs relationships
pub fn network_diagram(tracker: &Tracker) -> GraphData {
    if tracker.is_empty() {
        return GraphData::new();
    }

    let agent_names = tracker.get_working_agent_names();
    if agent_names.is_empty() {
        return GraphData::new();
    }

    let mut builder = GraphBuilder::new();

    // Process each agent (component)
    for agent_name in agent_names {
        // Create component node
        builder.get_or_create_node(agent_name, NodeType::Component);

        // Get what this agent wants
        let wants = tracker.get_agent_wants(agent_name.clone());

        for want_behavior in wants {
            // Create behavior node
            builder.get_or_create_node(&want_behavior, NodeType::Behavior);

            // Create link from component to behavior (wants relationship)
            // We'll determine if it's satisfied after resolving
            if !builder.link_exists(agent_name, &want_behavior, LinkType::Wants) {
                builder.links.push(GraphLink {
                    source: agent_name.clone(),
                    target: want_behavior.clone(),
                    link_type: LinkType::Wants,
                    satisfied: true, // Will be updated after resolution
                });
            }

            // Resolve the behavior to find providers and nested relationships
            let resolution = tracker.resolve(&want_behavior);
            let has_satisfied_providers = !resolution.get_satisfying_offers().is_empty();

            // Update the wants link based on whether there are satisfied providers
            if let Some(idx) = builder.find_link(agent_name, &want_behavior, LinkType::Wants) {
                builder.links[idx].satisfied = has_satisfied_providers;
            }

            // Update behavior node satisfaction
            if !has_satisfied_providers {
                if let Some(&idx) = builder.node_map.get(&want_behavior) {
                    builder.nodes[idx].satisfied = false;
                }
            }

            builder.process_resolution(&want_behavior, &resolution);
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior};

    #[test]
    fn test_empty_tracker() {
        let tracker = Tracker::new();
        let graph = network_diagram(&tracker);
        assert!(graph.is_empty());
    }

    #[test]
    fn test_simple_agent() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));

        let graph = network_diagram(&tracker);

        // Should have one component node
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "a1");
        assert_eq!(graph.nodes[0].node_type, NodeType::Component);
    }

    #[test]
    fn test_agent_with_wants() {
        let mut tracker = Tracker::new();

        // a1 wants b1, a2 provides b1
        let mut a1 = Agent::new("a1".to_string());
        a1.add_want(Behavior::new("b1".to_string()));
        tracker.add_agent(a1);
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let graph = network_diagram(&tracker);

        // Should have: a1 (component), a2 (component), b1 (behavior)
        assert_eq!(graph.nodes.len(), 3);

        // Should have links: a1 -> b1 (wants), b1 -> a2 (provides)
        assert!(graph.links.len() >= 2);

        // Check the wants link is satisfied
        let wants_link = graph
            .links
            .iter()
            .find(|l| l.link_type == LinkType::Wants)
            .unwrap();
        assert!(wants_link.satisfied);
    }

    #[test]
    fn test_unsatisfied_want() {
        let mut tracker = Tracker::new();

        // a1 wants b1, but nothing provides b1
        let mut a1 = Agent::new("a1".to_string());
        a1.add_want(Behavior::new("b1".to_string()));
        tracker.add_agent(a1);

        let graph = network_diagram(&tracker);

        // Should have: a1 (component), b1 (behavior)
        assert_eq!(graph.nodes.len(), 2);

        // Check the wants link is unsatisfied
        let wants_link = graph
            .links
            .iter()
            .find(|l| l.link_type == LinkType::Wants)
            .unwrap();
        assert!(!wants_link.satisfied);

        // Check the behavior node is unsatisfied
        let behavior_node = graph.nodes.iter().find(|n| n.id == "b1").unwrap();
        assert!(!behavior_node.satisfied);
    }
}
