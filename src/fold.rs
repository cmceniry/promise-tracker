//! Folding rules for the promise graph.
//!
//! The raw graph from [`crate::promise_graph`] draws every promise as its own
//! line. On a contract of any size the lines are mostly the same few patterns
//! repeated: an agent meeting its own conditions, a provider everyone leans
//! on, several promises between the same pair. Each rule here takes one
//! pattern off the canvas and says what it took, so the view can show the
//! count and a reader can turn the rule off.
//!
//! The rules never hide a broken promise's status: a folded self promise that
//! is not kept still counts against its node, a hub that is not kept turns the
//! badge on every consumer, and a bundle with a broken promise inside is drawn
//! broken.

use crate::promise_graph::{PromiseGraphData, PromiseKind, PromiseNodeKind, UnsatisfiedWant};
use crate::Tracker;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Which rules to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldOptions {
    /// Mark a collective's node as one and count its members. The tracker
    /// folds a collective before resolving, so this is a labeling rule: there
    /// is no unfolded graph to show when it is off.
    pub collectives: bool,
    /// A provider with at least this many promises leaned on becomes a hub:
    /// its incoming lines fold into a badge on each consumer. None turns the
    /// rule off.
    pub hub_threshold: Option<usize>,
    /// One line per ordered pair of agents, labelled with the count.
    pub bundle: bool,
    /// Promises an agent makes to itself stay inside the node as a count.
    pub own_inside: bool,
}

impl Default for FoldOptions {
    fn default() -> Self {
        FoldOptions {
            collectives: true,
            hub_threshold: Some(15),
            bundle: true,
            own_inside: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldedNodeKind {
    Agent,
    Collective,
    Hub,
    Missing,
}

/// A hub this node leans on, in place of the lines that used to say so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Badge {
    pub hub: String,
    /// Every promise this node wants from the hub is kept.
    pub satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldedNode {
    pub id: String,
    pub label: String,
    pub kind: FoldedNodeKind,
    /// A collective's members, flattened through nested collectives.
    pub members: Vec<String>,
    /// Promises this node makes to itself, folded inside.
    pub own: usize,
    /// How many of those are not kept.
    pub own_unsatisfied: usize,
    /// Promises other agents lean on this node for, before any hub folding.
    pub incoming: usize,
    pub badges: Vec<Badge>,
}

/// One promise inside a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldedPromise {
    pub behavior: String,
    pub kind: PromiseKind,
    pub satisfied: bool,
}

/// A line between two agents: one promise, or a bundle of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldedEdge {
    pub source: String,
    pub target: String,
    pub promises: Vec<FoldedPromise>,
    /// How many of the promises are not kept.
    pub unsatisfied: usize,
    pub label: String,
}

impl FoldedEdge {
    pub fn count(&self) -> usize {
        self.promises.len()
    }
}

/// What the rules took off the canvas, for the panel that controls them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FoldStats {
    /// Promises in the raw graph.
    pub promises: usize,
    /// Self promises folded into their nodes.
    pub own_folded: usize,
    /// Lines into hubs replaced by badges.
    pub hub_lines_folded: usize,
    /// Lines saved by bundling.
    pub bundled_away: usize,
    /// Hubs, with how many promises lean on each.
    pub hubs: Vec<(String, usize)>,
    /// Providers by promises leaned on, most first, hubs or not. For the
    /// threshold control.
    pub candidates: Vec<(String, usize)>,
    pub nodes_drawn: usize,
    pub lines_drawn: usize,
    pub badges_drawn: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FoldedGraph {
    pub nodes: Vec<FoldedNode>,
    pub edges: Vec<FoldedEdge>,
    pub unsatisfied: Vec<UnsatisfiedWant>,
    pub stats: FoldStats,
}

impl FoldedGraph {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// How many providers to list for the threshold control.
const CANDIDATES: usize = 8;

/// The label on a line.
fn edge_label(promises: &[FoldedPromise], unsatisfied: usize) -> String {
    match promises {
        [one] => one.behavior.clone(),
        many if unsatisfied == 0 => format!("\u{00d7}{}", many.len()),
        many => format!("\u{00d7}{} ({}\u{2717})", many.len(), unsatisfied),
    }
}

/// Apply the rules to a raw promise graph.
///
/// The tracker is only consulted for which nodes are collectives and who
/// their members are.
pub fn fold(graph: &PromiseGraphData, tracker: &Tracker, options: FoldOptions) -> FoldedGraph {
    if graph.is_empty() {
        return FoldedGraph::default();
    }

    let mut stats = FoldStats {
        promises: graph.edges.len(),
        ..FoldStats::default()
    };

    // Nodes, in the raw order.
    let mut nodes: Vec<FoldedNode> = graph
        .nodes
        .iter()
        .map(|n| {
            let members = if options.collectives && n.kind == PromiseNodeKind::Agent {
                tracker.collective_members(&n.id).unwrap_or_default()
            } else {
                vec![]
            };
            FoldedNode {
                id: n.id.clone(),
                label: n.label.clone(),
                kind: match n.kind {
                    PromiseNodeKind::Missing => FoldedNodeKind::Missing,
                    PromiseNodeKind::Agent if !members.is_empty() => FoldedNodeKind::Collective,
                    PromiseNodeKind::Agent => FoldedNodeKind::Agent,
                },
                members,
                own: 0,
                own_unsatisfied: 0,
                incoming: 0,
                badges: vec![],
            }
        })
        .collect();
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();

    // Rule: own promises inside. Count every self promise on its node, and
    // drop it from the lines when the rule is on.
    let mut lines: Vec<(usize, usize, FoldedPromise)> = vec![];
    for e in &graph.edges {
        let (Some(&s), Some(&t)) = (index.get(&e.source), index.get(&e.target)) else {
            continue;
        };
        let promise = FoldedPromise {
            behavior: e.behavior.clone(),
            kind: e.kind,
            satisfied: e.satisfied,
        };
        if s == t {
            nodes[s].own += 1;
            if !e.satisfied {
                nodes[s].own_unsatisfied += 1;
            }
            if options.own_inside {
                stats.own_folded += 1;
                continue;
            }
        } else {
            nodes[t].incoming += 1;
        }
        lines.push((s, t, promise));
    }

    // Who everyone leans on, hubs or not.
    let mut by_incoming: Vec<(String, usize)> = nodes
        .iter()
        .filter(|n| n.kind != FoldedNodeKind::Missing && n.incoming > 0)
        .map(|n| (n.id.clone(), n.incoming))
        .collect();
    by_incoming.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    stats.candidates = by_incoming.iter().take(CANDIDATES).cloned().collect();

    // Rule: hubs as badges. A missing behavior is never a hub: its lines are
    // the broken promises the graph exists to show.
    if let Some(threshold) = options.hub_threshold {
        for node in nodes.iter_mut() {
            if node.kind != FoldedNodeKind::Missing && node.incoming >= threshold {
                node.kind = FoldedNodeKind::Hub;
                stats.hubs.push((node.id.clone(), node.incoming));
            }
        }
        if !stats.hubs.is_empty() {
            // (source, hub) -> every promise kept
            let mut badges: BTreeMap<(usize, usize), bool> = BTreeMap::new();
            lines.retain(|(s, t, p)| {
                if nodes[*t].kind == FoldedNodeKind::Hub {
                    let kept = badges.entry((*s, *t)).or_insert(true);
                    *kept &= p.satisfied;
                    stats.hub_lines_folded += 1;
                    false
                } else {
                    true
                }
            });
            for ((s, t), satisfied) in badges {
                let hub = nodes[t].id.clone();
                nodes[s].badges.push(Badge { hub, satisfied });
            }
        }
    }

    // Rule: bundle parallel promises. Lines are grouped by pair in first-seen
    // order either way, so turning the rule off only splits the groups.
    let mut groups: Vec<((usize, usize), Vec<FoldedPromise>)> = vec![];
    let mut group_index: HashMap<(usize, usize), usize> = HashMap::new();
    for (s, t, p) in lines {
        match group_index.get(&(s, t)) {
            Some(&i) => groups[i].1.push(p),
            None => {
                group_index.insert((s, t), groups.len());
                groups.push(((s, t), vec![p]));
            }
        }
    }
    let mut edges: Vec<FoldedEdge> = vec![];
    for ((s, t), promises) in groups {
        let source = nodes[s].id.clone();
        let target = nodes[t].id.clone();
        if options.bundle {
            let unsatisfied = promises.iter().filter(|p| !p.satisfied).count();
            stats.bundled_away += promises.len() - 1;
            edges.push(FoldedEdge {
                label: edge_label(&promises, unsatisfied),
                source,
                target,
                promises,
                unsatisfied,
            });
        } else {
            for p in promises {
                let unsatisfied = usize::from(!p.satisfied);
                edges.push(FoldedEdge {
                    source: source.clone(),
                    target: target.clone(),
                    label: p.behavior.clone(),
                    promises: vec![p],
                    unsatisfied,
                });
            }
        }
    }

    stats.nodes_drawn = nodes.len();
    stats.lines_drawn = edges.len();
    stats.badges_drawn = nodes.iter().map(|n| n.badges.len()).sum();

    FoldedGraph {
        nodes,
        edges,
        unsatisfied: graph.unsatisfied.clone(),
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior, SuperAgent};
    use crate::promise_graph::promise_graph;

    fn node<'a>(g: &'a FoldedGraph, id: &str) -> &'a FoldedNode {
        g.nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("no node {} in {:#?}", id, g.nodes))
    }

    fn edge<'a>(g: &'a FoldedGraph, source: &str, target: &str) -> &'a FoldedEdge {
        g.edges
            .iter()
            .find(|e| e.source == source && e.target == target)
            .unwrap_or_else(|| panic!("no edge {} -> {} in {:#?}", source, target, g.edges))
    }

    /// Four consumers of one provider, one of them twice, plus a self promise.
    fn sample() -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("b1"),
            Behavior::build("b2"),
            Behavior::build("own").with_conditions(vec!["b1".into()]),
        ]));
        t.add_agent(Agent::build("q").with_provides(vec![Behavior::build("own2")]));
        for name in ["c1", "c2", "c3"] {
            t.add_agent(Agent::build(name).with_wants(vec![Behavior::build("b1")]));
        }
        t.add_agent(
            Agent::build("c4").with_wants(vec![Behavior::build("b1"), Behavior::build("b2")]),
        );
        t.add_agent(Agent::build("c5").with_wants(vec![Behavior::build("own")]));
        t
    }

    #[test]
    fn empty_graph() {
        let t = Tracker::new();
        let g = fold(&promise_graph(&t), &t, FoldOptions::default());
        assert!(g.is_empty());
    }

    #[test]
    fn nothing_on_leaves_every_promise_a_line() {
        let t = sample();
        let raw = promise_graph(&t);
        let g = fold(
            &raw,
            &t,
            FoldOptions {
                collectives: false,
                hub_threshold: None,
                bundle: false,
                own_inside: false,
            },
        );
        assert_eq!(g.edges.len(), raw.edges.len());
        assert!(g.edges.iter().all(|e| e.count() == 1));
        assert_eq!(g.stats.own_folded, 0);
        assert_eq!(g.stats.bundled_away, 0);
        assert!(g.stats.hubs.is_empty());
        // The self promise is still counted on its node.
        assert_eq!(node(&g, "p").own, 1);
    }

    #[test]
    fn own_promises_fold_into_the_node() {
        let t = sample();
        let g = fold(
            &promise_graph(&t),
            &t,
            FoldOptions {
                hub_threshold: None,
                bundle: false,
                ..FoldOptions::default()
            },
        );
        assert!(g.edges.iter().all(|e| e.source != e.target));
        assert_eq!(g.stats.own_folded, 1);
        let p = node(&g, "p");
        assert_eq!(p.own, 1);
        assert_eq!(p.own_unsatisfied, 0);
        // p is leaned on for b1 x4, b2 x1 and own x1 from others.
        assert_eq!(p.incoming, 6);
    }

    #[test]
    fn bundling_makes_one_line_per_pair() {
        let t = sample();
        let g = fold(
            &promise_graph(&t),
            &t,
            FoldOptions {
                hub_threshold: None,
                ..FoldOptions::default()
            },
        );
        let e = edge(&g, "c4", "p");
        assert_eq!(e.count(), 2);
        assert_eq!(e.label, "\u{00d7}2");
        assert_eq!(edge(&g, "c1", "p").label, "b1");
        assert_eq!(g.stats.bundled_away, 1);
    }

    #[test]
    fn a_hub_folds_its_lines_into_badges() {
        let t = sample();
        let g = fold(
            &promise_graph(&t),
            &t,
            FoldOptions {
                hub_threshold: Some(5),
                ..FoldOptions::default()
            },
        );
        let p = node(&g, "p");
        assert_eq!(p.kind, FoldedNodeKind::Hub);
        assert_eq!(g.stats.hubs, vec![("p".to_string(), 6)]);
        assert!(g.edges.iter().all(|e| e.target != "p"));
        assert_eq!(g.stats.hub_lines_folded, 6);
        let c4 = node(&g, "c4");
        assert_eq!(
            c4.badges,
            vec![Badge {
                hub: "p".into(),
                satisfied: true
            }]
        );
        assert_eq!(g.stats.badges_drawn, 5);
        assert_eq!(g.stats.candidates[0], ("p".to_string(), 6));
        // Below the threshold nothing changes.
        let g = fold(
            &promise_graph(&t),
            &t,
            FoldOptions {
                hub_threshold: Some(7),
                ..FoldOptions::default()
            },
        );
        assert_eq!(node(&g, "p").kind, FoldedNodeKind::Agent);
        assert!(g.stats.hubs.is_empty());
    }

    #[test]
    fn a_broken_promise_marks_its_badge_and_bundle() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("ok"),
            Behavior::build("bad").with_conditions(vec!["nothing".into()]),
        ]));
        t.add_agent(
            Agent::build("c").with_wants(vec![Behavior::build("ok"), Behavior::build("bad")]),
        );
        let raw = promise_graph(&t);

        let g = fold(
            &raw,
            &t,
            FoldOptions {
                hub_threshold: None,
                ..FoldOptions::default()
            },
        );
        let e = edge(&g, "c", "p");
        assert_eq!(e.unsatisfied, 1);
        assert_eq!(e.label, "\u{00d7}2 (1\u{2717})");
        // The missing behavior keeps its own line and is never a hub.
        assert_eq!(node(&g, "missing:nothing").kind, FoldedNodeKind::Missing);

        let g = fold(
            &raw,
            &t,
            FoldOptions {
                hub_threshold: Some(1),
                ..FoldOptions::default()
            },
        );
        assert_eq!(
            node(&g, "c").badges,
            vec![Badge {
                hub: "p".into(),
                satisfied: false
            }]
        );
        assert_eq!(node(&g, "missing:nothing").kind, FoldedNodeKind::Missing);
        assert!(g.edges.iter().any(|e| e.target == "missing:nothing"));
    }

    #[test]
    fn a_collective_is_marked_with_its_members() {
        let mut t = Tracker::new();
        t.add_superagent(
            SuperAgent::new("team".into())
                .with_agent("m1")
                .with_agent("m2"),
        );
        t.add_agent(Agent::build("m1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("m2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("c").with_wants(vec![Behavior::build("b1")]));

        let g = fold(&promise_graph(&t), &t, FoldOptions::default());
        let team = node(&g, "team");
        assert_eq!(team.kind, FoldedNodeKind::Collective);
        assert_eq!(team.members, vec!["m1".to_string(), "m2".to_string()]);

        let g = fold(
            &promise_graph(&t),
            &t,
            FoldOptions {
                collectives: false,
                ..FoldOptions::default()
            },
        );
        assert_eq!(node(&g, "team").kind, FoldedNodeKind::Agent);
        assert!(node(&g, "team").members.is_empty());
    }
}
