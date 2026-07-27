//! Edge-as-promise graph generation.
//!
//! Generates graph data where agents are nodes and every promise is an
//! agent-to-agent edge labeled with its behavior. Behaviors that nobody
//! offers become dashed "missing" ghost nodes so unmet needs stay visible
//! in the graph.

use crate::resolve::{Offer, Resolution};
use crate::Tracker;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// What relationship an edge represents: a top-level want, or a condition
/// a provider needs in order to keep one of its promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromiseKind {
    Want,
    Condition,
}

/// Node kind: a real agent, or a ghost standing in for a behavior with no offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromiseNodeKind {
    Agent,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromiseNode {
    pub id: String,
    pub label: String,
    pub kind: PromiseNodeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromiseEdge {
    pub source: String,
    pub target: String,
    pub behavior: String,
    pub kind: PromiseKind,
    pub satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnsatisfiedWant {
    pub agent: String,
    pub behavior: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromiseGraphData {
    pub nodes: Vec<PromiseNode>,
    pub edges: Vec<PromiseEdge>,
    pub unsatisfied: Vec<UnsatisfiedWant>,
}

impl PromiseGraphData {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Prefix for ghost node ids. An agent literally named "missing:x" would
/// collide; accepted for now.
fn ghost_id(behavior: &str) -> String {
    format!("missing:{}", behavior)
}

struct PromiseGraphBuilder {
    nodes: BTreeMap<String, PromiseNode>,
    // (source, target, behavior, kind) -> satisfied; satisfied merges by OR
    // since the same provider can offer the same behavior both conditionally
    // and unconditionally, and one green edge is the correct reading.
    edges: BTreeMap<(String, String, String, PromiseKind), bool>,
    unsatisfied: BTreeSet<UnsatisfiedWant>,
}

impl PromiseGraphBuilder {
    fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            unsatisfied: BTreeSet::new(),
        }
    }

    fn add_agent_node(&mut self, name: &str) {
        self.nodes
            .entry(name.to_string())
            .or_insert_with(|| PromiseNode {
                id: name.to_string(),
                label: name.to_string(),
                kind: PromiseNodeKind::Agent,
            });
    }

    fn add_ghost_node(&mut self, behavior: &str) -> String {
        let id = ghost_id(behavior);
        self.nodes.entry(id.clone()).or_insert_with(|| PromiseNode {
            id: id.clone(),
            label: format!("missing: {}", behavior),
            kind: PromiseNodeKind::Missing,
        });
        id
    }

    fn merge_edge(
        &mut self,
        source: &str,
        target: &str,
        behavior: &str,
        kind: PromiseKind,
        satisfied: bool,
    ) {
        let entry = self
            .edges
            .entry((
                source.to_string(),
                target.to_string(),
                behavior.to_string(),
                kind,
            ))
            .or_insert(false);
        *entry |= satisfied;
    }

    /// `requirer` needs the resolution's behavior: emit one edge per offer,
    /// or a ghost edge when there are no offers at all.
    fn emit_requirement(&mut self, requirer: &str, kind: PromiseKind, resolution: &Resolution) {
        let behavior = resolution.get_behavior_name();
        let satisfied = resolution.get_satisfying_offers();
        let unsatisfied = resolution.get_unsatisfying_offers();

        if satisfied.is_empty() && unsatisfied.is_empty() {
            let ghost = self.add_ghost_node(behavior);
            self.merge_edge(requirer, &ghost, behavior, kind, false);
            return;
        }
        for offer in satisfied {
            self.emit_offer(requirer, kind, behavior, offer, true);
        }
        for offer in unsatisfied {
            self.emit_offer(requirer, kind, behavior, offer, false);
        }
    }

    fn emit_offer(
        &mut self,
        requirer: &str,
        kind: PromiseKind,
        behavior: &str,
        offer: &Offer,
        satisfied: bool,
    ) {
        let provider = offer.get_agent_name();
        self.add_agent_node(provider);
        self.merge_edge(requirer, provider, behavior, kind, satisfied);
        // Conditions become edges from the provider: it needs them to keep
        // its promise. Unsatisfying offers still recurse — their satisfied
        // condition subtrees draw green.
        for condition in offer.get_resolved_conditions() {
            self.emit_requirement(provider, PromiseKind::Condition, condition);
        }
    }

    fn build(self) -> PromiseGraphData {
        PromiseGraphData {
            nodes: self.nodes.into_values().collect(),
            edges: self
                .edges
                .into_iter()
                .map(|((source, target, behavior, kind), satisfied)| PromiseEdge {
                    source,
                    target,
                    behavior,
                    kind,
                    satisfied,
                })
                .collect(),
            unsatisfied: self.unsatisfied.into_iter().collect(),
        }
    }
}

/// Generate edge-as-promise graph data from a Tracker.
///
/// For each working agent's want, every offer becomes an edge from the
/// wanter to the offering agent; each offer's conditions become edges from
/// that provider to whoever satisfies them, recursively. Wants that resolve
/// to nothing are listed in `unsatisfied` for the status strip.
pub fn promise_graph(tracker: &Tracker) -> PromiseGraphData {
    if tracker.is_empty() {
        return PromiseGraphData::default();
    }

    let mut builder = PromiseGraphBuilder::new();

    for agent_name in tracker.get_working_agent_names() {
        builder.add_agent_node(agent_name);

        let mut wants: Vec<String> = tracker.get_agent_wants(agent_name.clone()).into_iter().collect();
        wants.sort();

        for want in wants {
            let resolution = tracker.resolve(&want);
            if !resolution.is_satisfied() {
                builder.unsatisfied.insert(UnsatisfiedWant {
                    agent: agent_name.clone(),
                    behavior: want.clone(),
                });
            }
            builder.emit_requirement(agent_name, PromiseKind::Want, &resolution);
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior, SuperAgent};

    fn edge(
        source: &str,
        target: &str,
        behavior: &str,
        kind: PromiseKind,
        satisfied: bool,
    ) -> PromiseEdge {
        PromiseEdge {
            source: source.to_string(),
            target: target.to_string(),
            behavior: behavior.to_string(),
            kind,
            satisfied,
        }
    }

    fn node_ids(graph: &PromiseGraphData) -> Vec<&str> {
        graph.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn test_empty_tracker() {
        let tracker = Tracker::new();
        let graph = promise_graph(&tracker);
        assert!(graph.is_empty());
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_simple_satisfied_want() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker);

        assert_eq!(node_ids(&graph), vec!["a1", "a2"]);
        assert_eq!(
            graph.edges,
            vec![edge("a1", "a2", "b1", PromiseKind::Want, true)]
        );
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_multi_provider_want() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker);

        // Three providers = three parallel edges from the wanter.
        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a2", "b1", PromiseKind::Want, true),
                edge("a1", "a3", "b1", PromiseKind::Want, true),
                edge("a1", "a4", "b1", PromiseKind::Want, true),
            ]
        );
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_unresolvable_want_ghost() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker);

        assert_eq!(node_ids(&graph), vec!["a1", "missing:b1"]);
        let ghost = graph.nodes.iter().find(|n| n.id == "missing:b1").unwrap();
        assert_eq!(ghost.kind, PromiseNodeKind::Missing);
        assert_eq!(ghost.label, "missing: b1");
        assert_eq!(
            graph.edges,
            vec![edge("a1", "missing:b1", "b1", PromiseKind::Want, false)]
        );
        assert_eq!(
            graph.unsatisfied,
            vec![UnsatisfiedWant {
                agent: "a1".to_string(),
                behavior: "b1".to_string()
            }]
        );
    }

    #[test]
    fn test_condition_chain() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b2").with_conditions(vec!["b3".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b3")]));

        let graph = promise_graph(&tracker);

        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a2", "b1", PromiseKind::Want, true),
                edge("a2", "a3", "b2", PromiseKind::Condition, true),
                edge("a3", "a4", "b3", PromiseKind::Condition, true),
            ]
        );
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_unmet_condition() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));

        let graph = promise_graph(&tracker);

        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a2", "b1", PromiseKind::Want, false),
                edge("a2", "missing:b2", "b2", PromiseKind::Condition, false),
            ]
        );
        // The strip lists the wanter, not the conditioning provider.
        assert_eq!(
            graph.unsatisfied,
            vec![UnsatisfiedWant {
                agent: "a1".to_string(),
                behavior: "b1".to_string()
            }]
        );
    }

    #[test]
    fn test_mixed_offers() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["bx".to_string()]),
        ]));

        let graph = promise_graph(&tracker);

        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a2", "b1", PromiseKind::Want, true),
                edge("a1", "a3", "b1", PromiseKind::Want, false),
                edge("a3", "missing:bx", "bx", PromiseKind::Condition, false),
            ]
        );
        // The want overall is satisfied via a2, so the strip stays empty.
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_reciprocal_promises() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![Behavior::build("b2")]),
        );
        tracker.add_agent(
            Agent::build("a2")
                .with_wants(vec![Behavior::build("b2")])
                .with_provides(vec![Behavior::build("b1")]),
        );

        let graph = promise_graph(&tracker);

        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a2", "b1", PromiseKind::Want, true),
                edge("a2", "a1", "b2", PromiseKind::Want, true),
            ]
        );
    }

    #[test]
    fn test_dedup() {
        let mut tracker = Tracker::new();
        // Two wanters both want b1 and bshared; both behaviors are provided
        // by p conditional on the same sub-behavior c.
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1"), Behavior::build("bshared")]),
        );
        tracker.add_agent(
            Agent::build("a2")
                .with_wants(vec![Behavior::build("b1"), Behavior::build("bshared")]),
        );
        tracker.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["c".to_string()]),
            Behavior::build("bshared").with_conditions(vec!["c".to_string()]),
        ]));
        tracker.add_agent(Agent::build("pc").with_provides(vec![Behavior::build("c")]));

        let graph = promise_graph(&tracker);

        // The shared condition edge appears exactly once despite being
        // reached through four resolution paths.
        let condition_edges: Vec<&PromiseEdge> = graph
            .edges
            .iter()
            .filter(|e| e.kind == PromiseKind::Condition)
            .collect();
        assert_eq!(
            condition_edges,
            vec![&edge("p", "pc", "c", PromiseKind::Condition, true)]
        );
        assert_eq!(graph.edges.len(), 5); // 4 want edges + 1 condition edge
    }

    #[test]
    fn test_satisfied_or_merge() {
        let mut tracker = Tracker::new();
        // p provides b1 twice: unconditionally, and with an unmet condition.
        // The wanter should get a single green edge to p.
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("b1"),
            Behavior::build("b1").with_conditions(vec!["nope".to_string()]),
        ]));

        let graph = promise_graph(&tracker);

        let want_edges: Vec<&PromiseEdge> = graph
            .edges
            .iter()
            .filter(|e| e.kind == PromiseKind::Want)
            .collect();
        assert_eq!(
            want_edges,
            vec![&edge("a1", "p", "b1", PromiseKind::Want, true)]
        );
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_superagent_flattening() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        tracker.add_superagent(
            SuperAgent::new("sa1".to_string())
                .with_agent("a1")
                .with_agent("a2"),
        );
        tracker.add_agent(Agent::build("w").with_wants(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker);

        // Edges reference the flattened working name, not inner agents.
        assert_eq!(node_ids(&graph), vec!["sa1", "w"]);
        assert_eq!(
            graph.edges,
            vec![edge("w", "sa1", "b1", PromiseKind::Want, true)]
        );
    }
}
