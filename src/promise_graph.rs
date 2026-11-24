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

    /// Drop the promises an agent makes to itself, where the wanter and the
    /// provider are the same agent.
    ///
    /// Nodes and `unsatisfied` are deliberately left alone. Conditions are
    /// emitted as edges *from* the provider, so whatever a hidden self promise
    /// depended on from other agents survives on its own; and an agent whose
    /// only link was to itself stays on the canvas rather than disappearing,
    /// which also keeps this filter from touching agents that were already
    /// isolated before it ran.
    pub fn without_self_promises(mut self) -> PromiseGraphData {
        self.edges.retain(|e| e.source != e.target);
        self
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
        // An offer answering a gathered row is drawn under the concrete
        // behavior it keeps; the goal itself names none.
        for offer in satisfied {
            let kept = offer.get_behavior().unwrap_or(behavior);
            self.emit_offer(requirer, kind, kept, offer, true);
        }
        for offer in unsatisfied {
            let kept = offer.get_behavior().unwrap_or(behavior);
            self.emit_offer(requirer, kind, kept, offer, false);
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

    /// Every node some edge starts or ends at.
    fn endpoints(&self) -> BTreeSet<String> {
        self.edges
            .keys()
            .flat_map(|(source, target, _, _)| [source.clone(), target.clone()])
            .collect()
    }

    fn build(self) -> PromiseGraphData {
        PromiseGraphData {
            nodes: self.nodes.into_values().collect(),
            edges: self
                .edges
                .into_iter()
                .map(
                    |((source, target, behavior, kind), satisfied)| PromiseEdge {
                        source,
                        target,
                        behavior,
                        kind,
                        satisfied,
                    },
                )
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
    promise_graph_with(tracker, false)
}

/// [`promise_graph`], with the "Fold: Unneeded alternatives" display option.
///
/// When `fold_alternatives` is on, every want is drawn from its resolution
/// without the offers that fail to keep a behavior another offer already
/// keeps ([`Resolution::without_unneeded_alternatives`]). A promise two
/// chains share is still drawn when either chain that reaches it is.
///
/// An agent that was only on the canvas for such offers goes with them: one
/// with a promise in the full graph and none in the folded one has no node.
/// An agent with no promises to begin with keeps its node, as it does with
/// the option off. `unsatisfied` is the same either way.
pub fn promise_graph_with(tracker: &Tracker, fold_alternatives: bool) -> PromiseGraphData {
    if tracker.is_empty() {
        return PromiseGraphData::default();
    }

    let mut builder = PromiseGraphBuilder::new();
    // The graph as it is with the option off, kept beside the folded one
    // only to tell an agent the folding left alone from one that never had
    // a promise drawn.
    let mut unfolded = fold_alternatives.then(PromiseGraphBuilder::new);

    for agent_name in tracker.get_working_agent_names() {
        builder.add_agent_node(agent_name);

        let mut wants: Vec<String> = tracker
            .get_agent_wants(agent_name.clone())
            .into_iter()
            .collect();
        wants.sort();

        for want in wants {
            let resolution = tracker.resolve(&want);
            if !resolution.is_satisfied() {
                builder.unsatisfied.insert(UnsatisfiedWant {
                    agent: agent_name.clone(),
                    behavior: want.clone(),
                });
            }
            match unfolded.as_mut() {
                Some(unfolded) => {
                    unfolded.emit_requirement(agent_name, PromiseKind::Want, &resolution);
                    let folded = resolution.without_unneeded_alternatives();
                    builder.emit_requirement(agent_name, PromiseKind::Want, &folded);
                }
                None => builder.emit_requirement(agent_name, PromiseKind::Want, &resolution),
            }
        }
    }

    if let Some(unfolded) = unfolded {
        let had_promise = unfolded.endpoints();
        let has_promise = builder.endpoints();
        builder
            .nodes
            .retain(|id, _| has_promise.contains(id) || !had_promise.contains(id));
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
    fn test_open_want_draws_each_match_under_its_own_name() {
        // What an open want was: a gathered row on the promise that keeps
        // the want.
        let covering = || {
            Agent::build("c").with_provides(vec![Behavior::build("covered")
                .with_for_some(vec!["who".into()])
                .with_conditions(vec!["{{who}} backs up".into()])])
        };
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("covered")]));
        tracker.add_agent(covering());
        tracker.add_agent(Agent::build("b1").with_provides(vec![Behavior::build("b1 backs up")]));
        tracker.add_agent(Agent::build("b2").with_provides(vec![Behavior::build("b2 backs up")]));

        let graph = promise_graph(&tracker);
        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "c", "covered", PromiseKind::Want, true),
                edge("c", "b1", "b1 backs up", PromiseKind::Condition, true),
                edge("c", "b2", "b2 backs up", PromiseKind::Condition, true),
            ]
        );
        assert!(graph.unsatisfied.is_empty());

        // with nothing to match, the pattern itself is what is missing
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("covered")]));
        tracker.add_agent(covering());
        let graph = promise_graph(&tracker);
        assert_eq!(node_ids(&graph), vec!["a1", "c", "missing:{{who}} backs up"]);
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
            Agent::build("a1").with_wants(vec![Behavior::build("b1"), Behavior::build("bshared")]),
        );
        tracker.add_agent(
            Agent::build("a2").with_wants(vec![Behavior::build("b1"), Behavior::build("bshared")]),
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
    fn test_self_promise_edge() {
        let mut tracker = Tracker::new();
        // a1 both wants and provides b1, so it answers its own want.
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![Behavior::build("b1")]),
        );
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker);

        assert_eq!(
            graph.edges,
            vec![
                edge("a1", "a1", "b1", PromiseKind::Want, true),
                edge("a1", "a2", "b1", PromiseKind::Want, true),
            ]
        );
    }

    #[test]
    fn test_without_self_promises_drops_self_edge() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![Behavior::build("b1")]),
        );
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let graph = promise_graph(&tracker).without_self_promises();

        assert_eq!(
            graph.edges,
            vec![edge("a1", "a2", "b1", PromiseKind::Want, true)]
        );
        // The agent itself is still on the canvas.
        assert_eq!(node_ids(&graph), vec!["a1", "a2"]);
    }

    #[test]
    fn test_without_self_promises_keeps_downstream_conditions() {
        let mut tracker = Tracker::new();
        // a1 answers its own want, but only if a2 provides b2. Hiding the
        // self promise must not hide what it depends on from a2.
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![
                    Behavior::build("b1").with_conditions(vec!["b2".to_string()])
                ]),
        );
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));

        let graph = promise_graph(&tracker).without_self_promises();

        assert_eq!(
            graph.edges,
            vec![edge("a1", "a2", "b2", PromiseKind::Condition, true)]
        );
    }

    #[test]
    fn test_without_self_promises_keeps_ghosts_and_unsatisfied() {
        let mut tracker = Tracker::new();
        // The only offer is a1's own, and its condition is unmet.
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![
                    Behavior::build("b1").with_conditions(vec!["b2".to_string()])
                ]),
        );

        let graph = promise_graph(&tracker).without_self_promises();

        assert_eq!(node_ids(&graph), vec!["a1", "missing:b2"]);
        assert_eq!(
            graph.edges,
            vec![edge(
                "a1",
                "missing:b2",
                "b2",
                PromiseKind::Condition,
                false
            )]
        );
        assert_eq!(
            graph.unsatisfied,
            vec![UnsatisfiedWant {
                agent: "a1".to_string(),
                behavior: "b1".to_string(),
            }]
        );
    }

    #[test]
    fn test_without_self_promises_drops_self_conditions() {
        let mut tracker = Tracker::new();
        // p meets its own condition: that is a self promise of kind Condition.
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["c".to_string()]),
            Behavior::build("c"),
        ]));

        let graph = promise_graph(&tracker);
        assert!(graph
            .edges
            .contains(&edge("p", "p", "c", PromiseKind::Condition, true)));

        let graph = graph.without_self_promises();
        assert_eq!(
            graph.edges,
            vec![edge("a1", "p", "b1", PromiseKind::Want, true)]
        );
    }

    #[test]
    fn test_without_self_promises_is_a_noop_without_them() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".to_string()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b2")]));

        let graph = promise_graph(&tracker);

        // Equality here is what keeps the frontend memo from firing, and the
        // graph from re-laying out, when the toggle can change nothing.
        assert_eq!(graph.clone().without_self_promises(), graph);
    }

    #[test]
    fn test_without_self_promises_superagent() {
        let mut tracker = Tracker::new();
        // Flattening puts the wanter and the provider inside one agent.
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        tracker.add_superagent(
            SuperAgent::new("sa1".to_string())
                .with_agent("a1")
                .with_agent("a2"),
        );

        let graph = promise_graph(&tracker);
        assert_eq!(
            graph.edges,
            vec![edge("sa1", "sa1", "b1", PromiseKind::Want, true)]
        );

        let graph = graph.without_self_promises();
        assert!(graph.edges.is_empty());
        assert_eq!(node_ids(&graph), vec!["sa1"]);
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

    #[test]
    fn test_parameterized_promises_do_not_merge() {
        let mut tracker = Tracker::new();
        // One host promising execution to whoever asks, on terms naming the
        // asker. Only p1's binary is installed.
        tracker.add_agent(Agent::build("host").with_provides(vec![
            Behavior::build("process-execution/{{process}}")
                .with_conditions(vec!["binary-installed/{{process}}".to_string()]),
        ]));
        tracker.add_agent(
            Agent::build("p1").with_wants(vec![Behavior::build("process-execution/p1")]),
        );
        tracker.add_agent(
            Agent::build("p2").with_wants(vec![Behavior::build("process-execution/p2")]),
        );
        tracker.add_agent(
            Agent::build("pkg").with_provides(vec![Behavior::build("binary-installed/p1")]),
        );

        let graph = promise_graph(&tracker);

        // The host's two obligations are separate edges with separate verdicts.
        // Before names could carry a value they were one edge, drawn green
        // whenever *either* binary was installed.
        assert_eq!(
            graph.edges,
            vec![
                edge(
                    "host",
                    "missing:binary-installed/p2",
                    "binary-installed/p2",
                    PromiseKind::Condition,
                    false
                ),
                edge(
                    "host",
                    "pkg",
                    "binary-installed/p1",
                    PromiseKind::Condition,
                    true
                ),
                edge(
                    "p1",
                    "host",
                    "process-execution/p1",
                    PromiseKind::Want,
                    true
                ),
                edge(
                    "p2",
                    "host",
                    "process-execution/p2",
                    PromiseKind::Want,
                    false
                ),
            ]
        );
        assert_eq!(
            graph.unsatisfied,
            vec![UnsatisfiedWant {
                agent: "p2".to_string(),
                behavior: "process-execution/p2".to_string(),
            }]
        );
    }

    #[test]
    fn test_a_pattern_nobody_wants_draws_nothing() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("host").with_provides(vec![Behavior::build("run/{{process}}")]),
        );

        let graph = promise_graph(&tracker);

        // A promise with no promisee has no edge. The host is still on the
        // canvas; what it offers belongs to the offered view.
        assert_eq!(node_ids(&graph), vec!["host"]);
        assert!(graph.edges.is_empty());
    }

    // --- Fold: Unneeded alternatives ---------------------------------------

    fn conditional(name: &str, conditions: &[&str]) -> Behavior {
        Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
    }

    /// `x` wants `b1` and `b2`. `good` keeps `b1`; `alt` offers `b1` on `c`
    /// and `d` (nobody provides `d`) and keeps `b2` on `c`.
    fn shared_chain() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("x").with_wants(vec![Behavior::build("b1"), Behavior::build("b2")]),
        );
        tracker.add_agent(Agent::build("good").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("alt").with_provides(vec![
            conditional("b1", &["c", "d"]),
            conditional("b2", &["c"]),
        ]));
        tracker.add_agent(Agent::build("cprov").with_provides(vec![Behavior::build("c")]));
        tracker
    }

    #[test]
    fn test_fold_alternatives_off_is_the_plain_graph() {
        let tracker = shared_chain();
        assert_eq!(promise_graph_with(&tracker, false), promise_graph(&tracker));
        let graph = promise_graph(&tracker);
        assert_eq!(
            node_ids(&graph),
            vec!["alt", "cprov", "good", "missing:d", "x"]
        );
        assert_eq!(
            graph.edges,
            vec![
                edge("alt", "cprov", "c", PromiseKind::Condition, true),
                edge("alt", "missing:d", "d", PromiseKind::Condition, false),
                edge("x", "alt", "b1", PromiseKind::Want, false),
                edge("x", "alt", "b2", PromiseKind::Want, true),
                edge("x", "good", "b1", PromiseKind::Want, true),
            ]
        );
    }

    #[test]
    fn test_fold_alternatives_keeps_a_promise_a_kept_chain_still_reaches() {
        let tracker = shared_chain();
        let graph = promise_graph_with(&tracker, true);
        assert_eq!(node_ids(&graph), vec!["alt", "cprov", "good", "x"]);
        // alt -> cprov is shared: the broken offer of b1 is gone, the kept
        // offer of b2 still rests on it.
        assert_eq!(
            graph.edges,
            vec![
                edge("alt", "cprov", "c", PromiseKind::Condition, true),
                edge("x", "alt", "b2", PromiseKind::Want, true),
                edge("x", "good", "b1", PromiseKind::Want, true),
            ]
        );
        assert!(graph.unsatisfied.is_empty());
    }

    #[test]
    fn test_fold_alternatives_drops_an_agent_left_with_no_promise() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("x").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("good").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("alt").with_provides(vec![conditional("b1", &["c", "d"])]));
        tracker.add_agent(Agent::build("cprov").with_provides(vec![Behavior::build("c")]));
        // never had a promise drawn, with the option or without
        tracker.add_agent(Agent::build("idle").with_provides(vec![Behavior::build("unasked")]));

        let plain = promise_graph(&tracker);
        assert_eq!(
            node_ids(&plain),
            vec!["alt", "cprov", "good", "idle", "missing:d", "x"]
        );

        let folded = promise_graph_with(&tracker, true);
        // alt and cprov were only there for the offer that is gone, and the
        // ghost with them; idle stays, as isolated as it always was.
        assert_eq!(node_ids(&folded), vec!["good", "idle", "x"]);
        assert_eq!(
            folded.edges,
            vec![edge("x", "good", "b1", PromiseKind::Want, true)]
        );
        // every edge still ends at a node
        for e in &folded.edges {
            assert!(folded.nodes.iter().any(|n| n.id == e.source));
            assert!(folded.nodes.iter().any(|n| n.id == e.target));
        }
    }

    #[test]
    fn test_fold_alternatives_leaves_an_unsatisfied_want_its_offers() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("w").with_wants(vec![Behavior::build("top")]));
        tracker.add_agent(Agent::build("p").with_provides(vec![conditional("top", &["z1", "z2"])]));
        tracker.add_agent(Agent::build("az1").with_provides(vec![Behavior::build("z1")]));
        tracker.add_agent(Agent::build("ay1").with_provides(vec![conditional("z1", &["y1"])]));

        let plain = promise_graph(&tracker);
        let folded = promise_graph_with(&tracker, true);
        assert_eq!(folded.unsatisfied, plain.unsatisfied);
        assert_eq!(folded.unsatisfied.len(), 1);
        // ay1's failing offer of z1 goes, inside the broken chain too
        assert_eq!(node_ids(&folded), vec!["az1", "missing:z2", "p", "w"]);
        assert_eq!(
            folded.edges,
            vec![
                edge("p", "az1", "z1", PromiseKind::Condition, true),
                edge("p", "missing:z2", "z2", PromiseKind::Condition, false),
                edge("w", "p", "top", PromiseKind::Want, false),
            ]
        );
    }

    #[test]
    fn test_fold_alternatives_drops_a_collective_left_with_no_promise() {
        // A folded collective is one node: when its only promise is an
        // unneeded alternative it goes like any other agent, and nothing is
        // left pointing at it.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("x").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("good").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("m1").with_provides(vec![conditional("b1", &["d"])]));
        tracker.add_agent(Agent::build("m2"));
        tracker.add_superagent(
            SuperAgent::new(String::from("sa"))
                .with_agent("m1")
                .with_agent("m2"),
        );

        assert!(node_ids(&promise_graph(&tracker)).contains(&"sa"));
        let folded = promise_graph_with(&tracker, true);
        assert_eq!(node_ids(&folded), vec!["good", "x"]);

        let drawn = crate::fold::fold(&folded, &tracker, crate::fold::FoldOptions::default());
        assert_eq!(
            drawn.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["good", "x"]
        );
        assert_eq!(drawn.edges.len(), 1);
    }
}
