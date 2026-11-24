//! The contract's inventory, as data: what an agent provides, who provides
//! and who uses a behavior, which wants are unsatisfied and why, and which
//! promises nobody uses.
//!
//! These are the queries behind the CLI's `provides`, `providers`,
//! `check-unsatisfied --summary` and `unused`, and behind the UI's Inventory
//! tab, so the two read the contract the same way. Nothing here prints.
//!
//! SuperAgent folding is a property of the tracker
//! ([`Tracker::set_fold_collectives`]); the other display options travel in
//! [`QueryOptions`].
//!
//! # Terms
//!
//! - A behavior's **users** are the agents that want it, together with the
//!   agents with a promise conditioned on it (a parameterized condition counts
//!   when it could name the behavior).
//! - A **self-promise** is agent A providing X where A is X's only user. With
//!   [`QueryOptions::fold_self`] on these disappear from every list, and they
//!   are never reported as unused either way: A uses X.
//! - **Bundling** ([`QueryOptions::bundle`]) collapses rows that differ only
//!   by agent into one row whose `agents` lists them all. Unbundled, every
//!   such row names exactly one agent.
//! - An **unneeded alternative** is a promise that is not held, for a
//!   behavior something else keeps. With [`QueryOptions::fold_alternatives`]
//!   on these leave a behavior's providers ([`behavior_use`]) and an agent's
//!   provides ([`agent_provides`]); a behavior nobody keeps still shows every
//!   promise for it. It is the Inventory's reading of
//!   [`Resolution::without_unneeded_alternatives`], and it never touches
//!   [`unsatisfied`] or [`unused`].

use crate::components::{Behavior, Pattern};
use crate::resolve::Resolution;
use crate::{HoldsForAny, Tracker};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The display options that shape a query's lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryOptions {
    /// Drop self-promises everywhere; see the module docs.
    pub fold_self: bool,
    /// Collapse rows that differ only by agent; see the module docs.
    pub bundle: bool,
    /// Drop promises that are not kept where the behavior is kept anyway;
    /// see the module docs. Absent from older serialized options, where it
    /// is off.
    #[serde(default)]
    pub fold_alternatives: bool,
}

/// One condition on a promise, resolved against the tracker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionStatus {
    /// The behavior the condition needs, with any bindings substituted.
    pub behavior: String,
    /// Someone keeps it.
    pub held: bool,
}

/// One concrete promise an agent makes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvideRow {
    pub behavior: String,
    /// Empty for an unconditional promise.
    pub conditions: Vec<ConditionStatus>,
    /// Every condition holds (always true when there are none).
    pub held: bool,
}

/// One parameterized promise an agent makes. Its conditions name families
/// of behaviors too, so they are listed as written and not resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternProvideRow {
    pub pattern: String,
    pub conditions: Vec<String>,
}

/// What one agent provides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProvides {
    pub agent: String,
    /// Concrete promises, by behavior. A behavior promised more than once
    /// (say outright and under conditions) has a row for each.
    pub provides: Vec<ProvideRow>,
    /// Parameterized promises, by pattern.
    pub patterns: Vec<PatternProvideRow>,
}

/// Agents offering a behavior the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRow {
    /// One agent, or several when bundled.
    pub agents: Vec<String>,
    /// The parameterized promise the behavior matched, as written; None for a
    /// promise naming the behavior outright.
    pub pattern: Option<String>,
    pub conditions: Vec<ConditionStatus>,
    pub held: bool,
}

/// How a consumer uses a behavior.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ConsumerKind {
    /// The agent wants it.
    Want,
    /// One of the agent's promises is conditioned on it.
    Condition {
        /// That promise, as written.
        provide: String,
    },
}

/// Agents using a behavior the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerRow {
    /// One agent, or several when bundled.
    pub agents: Vec<String>,
    pub kind: ConsumerKind,
}

/// Who provides a behavior and who uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorUse {
    pub behavior: String,
    /// Someone keeps it.
    pub satisfied: bool,
    /// By agent.
    pub providers: Vec<ProviderRow>,
    /// Wants first, then conditions, each by agent.
    pub consumers: Vec<ConsumerRow>,
}

/// Why a want fails, at the bottom of the chain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CauseKind {
    /// Nobody promises the behavior.
    Unprovided,
    /// Someone promises it, but only by way of itself: the promise sits on a
    /// dependency loop (or a chain too deep to follow) and so keeps nothing.
    Cycle,
    /// A condition still holding a declared (`forSome`) variable that no
    /// promise names a value for.
    Unfound {
        /// The variables nothing names a value for, in name order.
        variables: Vec<String>,
        /// The promises that answered them while holding for any value of
        /// them. Empty when nothing answered at all.
        holds_for_any: Vec<HoldsForAny>,
    },
}

/// Where a failing condition sits: the promise that needs it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CauseSite {
    /// The agent making the promise.
    pub agent: String,
    /// The promise, as the want's resolution reached it.
    pub provide: String,
}

/// One leaf cause of an unsatisfied want.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Cause {
    /// The first behavior down the chain that nothing keeps.
    pub behavior: String,
    pub kind: CauseKind,
    /// None when the wanted behavior itself is the cause; otherwise the
    /// promise whose condition it is.
    pub needed_by: Option<CauseSite>,
}

/// A want nobody keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsatisfiedRow {
    /// One agent, or several when bundled.
    pub agents: Vec<String>,
    pub behavior: String,
    /// Sorted, without repeats.
    pub causes: Vec<Cause>,
}

/// Agents making an unused promise the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedProvider {
    /// One agent, or several when bundled.
    pub agents: Vec<String>,
    /// The promise's conditions, as written.
    pub conditions: Vec<String>,
}

/// A promised behavior nobody uses, with who promises it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedBehavior {
    /// The behavior, or the pattern as written.
    pub behavior: String,
    pub pattern: bool,
    pub providers: Vec<UnusedProvider>,
}

/// What an agent provides. None when the tracker has no such working agent.
pub fn agent_provides(
    tracker: &Tracker,
    agent: &str,
    options: QueryOptions,
) -> Option<AgentProvides> {
    let promises = tracker.get_agent_promises(agent)?;
    let index = Index::new(tracker);
    let mut resolver = Resolver::new(tracker);

    let mut provides: Vec<ProvideRow> = vec![];
    let mut patterns: Vec<PatternProvideRow> = vec![];
    let mut needed: Option<BTreeSet<String>> = None;
    for promise in &promises {
        if !promise.get_name_pattern().is_ground() {
            // A pattern folds away when everything it keeps that anyone uses
            // is a self-promise of this agent.
            if options.fold_self {
                let kept: Vec<&String> = needed
                    .get_or_insert_with(|| index.needed_names(&mut resolver))
                    .iter()
                    .filter(|n| promise.match_goal(n).is_some())
                    .collect();
                if !kept.is_empty() && kept.iter().all(|n| index.is_self_promise(agent, n)) {
                    continue;
                }
            }
            patterns.push(PatternProvideRow {
                pattern: promise.get_name().clone(),
                conditions: promise.get_conditions(),
            });
            continue;
        }
        if options.fold_self && index.is_self_promise(agent, promise.get_name()) {
            continue;
        }
        let conditions = resolver.conditions(promise);
        let held = conditions.iter().all(|c| c.held);
        // An unneeded alternative: this promise is not held, so if the
        // behavior is kept at all, some other offer keeps it.
        if options.fold_alternatives
            && !held
            && resolver.resolve(promise.get_name()).is_satisfied()
        {
            continue;
        }
        provides.push(ProvideRow {
            behavior: promise.get_name().clone(),
            held,
            conditions,
        });
    }
    provides.sort_by(|a, b| a.behavior.cmp(&b.behavior));
    provides.dedup();
    patterns.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    patterns.dedup();
    Some(AgentProvides {
        agent: agent.to_string(),
        provides,
        patterns,
    })
}

/// Who provides `behavior` and who uses it. None when nothing in the tracker
/// promises, wants or depends on it.
pub fn behavior_use(
    tracker: &Tracker,
    behavior: &str,
    options: QueryOptions,
) -> Option<BehaviorUse> {
    if !tracker.has_ground_behavior(behavior) {
        return None;
    }
    let index = Index::new(tracker);
    let mut resolver = Resolver::new(tracker);
    // A self-promise folds away the whole behavior: its only user keeps it
    // for itself, so whoever else promises it is not promising it to anyone.
    let folded_away = options.fold_self && {
        let users = index.users(behavior);
        users.len() == 1 && users.iter().all(|a| index.provides(a, behavior))
    };

    let mut providers: Vec<ProviderRow> = vec![];
    for (agent, promise) in index.promises() {
        let Some(bindings) = promise.match_goal(behavior) else {
            continue;
        };
        if folded_away {
            continue;
        }
        let conditions = resolver.conditions(&promise.instantiate(&bindings));
        let pattern = (!promise.get_name_pattern().is_ground()).then(|| promise.get_name().clone());
        let row = ProviderRow {
            agents: vec![agent.clone()],
            pattern,
            held: conditions.iter().all(|c| c.held),
            conditions,
        };
        if !providers.contains(&row) {
            providers.push(row);
        }
    }

    let mut consumers: Vec<ConsumerRow> = index
        .consumers(behavior)
        .into_iter()
        .filter(|(agent, _)| {
            !folded_away && !(options.fold_self && index.provides(agent, behavior))
        })
        .map(|(agent, kind)| ConsumerRow {
            agents: vec![agent],
            kind,
        })
        .collect();
    consumers.sort_by(|a, b| {
        (a.kind != ConsumerKind::Want, &a.agents, &a.kind).cmp(&(
            b.kind != ConsumerKind::Want,
            &b.agents,
            &b.kind,
        ))
    });

    // Unneeded alternatives: where some promise for the behavior is held,
    // the ones that are not go. With none held, every one stays.
    if options.fold_alternatives && providers.iter().any(|r| r.held) {
        providers.retain(|r| r.held);
    }

    if options.bundle {
        providers = bundle(providers, |r| &mut r.agents);
        consumers = bundle(consumers, |r| &mut r.agents);
    }
    Some(BehaviorUse {
        behavior: behavior.to_string(),
        satisfied: resolver.resolve(behavior).is_satisfied(),
        providers,
        consumers,
    })
}

/// Every want nobody keeps, by agent then behavior, each with the leaf
/// causes of its failure.
///
/// Want-level only: a conditional offer that fails while another provider
/// still keeps the want is not listed. A cause is the first behavior down a
/// chain of failing conditions that nothing keeps. Self folding does not
/// change this list: a want an agent keeps for itself is never unsatisfied.
pub fn unsatisfied(tracker: &Tracker, options: QueryOptions) -> Vec<UnsatisfiedRow> {
    let index = Index::new(tracker);
    let mut resolver = Resolver::new(tracker);
    let mut rows: Vec<UnsatisfiedRow> = vec![];
    for (agent, wants) in &index.wants {
        for want in wants {
            let resolution = resolver.resolve(want);
            if resolution.is_satisfied() {
                continue;
            }
            let mut causes: BTreeSet<Cause> = BTreeSet::new();
            leaf_causes(tracker, &resolution, None, &mut causes);
            rows.push(UnsatisfiedRow {
                agents: vec![agent.clone()],
                behavior: want.clone(),
                causes: causes.into_iter().collect(),
            });
        }
    }
    if options.bundle {
        rows = bundle(rows, |r| &mut r.agents);
    }
    rows
}

/// Every promised behavior nobody uses, by behavior then pattern, each with
/// the agents promising it.
///
/// A concrete behavior is unused when no agent wants it and no condition
/// anywhere needs it; one its own provider uses is used. A pattern is unused
/// when it matches no wanted or conditioned behavior, counting conditions as
/// resolution reaches them with bindings filled in. Self-promises are never
/// reported, with self folding or without: on, they are folded away; off,
/// their provider uses them.
pub fn unused(tracker: &Tracker, options: QueryOptions) -> Vec<UnusedBehavior> {
    let index = Index::new(tracker);
    let needed = index.needed_names(&mut Resolver::new(tracker));

    let mut groups: BTreeMap<(bool, String), Vec<UnusedProvider>> = BTreeMap::new();
    for (agent, promise) in index.promises() {
        let name = promise.get_name();
        let pattern = !promise.get_name_pattern().is_ground();
        let used = if pattern {
            needed.iter().any(|n| promise.match_goal(n).is_some())
        } else {
            !index.users(name).is_empty()
        };
        if used {
            continue;
        }
        let row = UnusedProvider {
            agents: vec![agent.clone()],
            conditions: promise.get_conditions(),
        };
        let providers = groups.entry((pattern, name.clone())).or_default();
        if !providers.contains(&row) {
            providers.push(row);
        }
    }

    groups
        .into_iter()
        .map(|((pattern, behavior), providers)| UnusedBehavior {
            behavior,
            pattern,
            providers: if options.bundle {
                bundle(providers, |r| &mut r.agents)
            } else {
                providers
            },
        })
        .collect()
}

/// Descend through an unsatisfied resolution to the behaviors nothing keeps.
fn leaf_causes(
    tracker: &Tracker,
    resolution: &Resolution,
    needed_by: Option<CauseSite>,
    out: &mut BTreeSet<Cause>,
) {
    let behavior = resolution.get_behavior_name();
    let offers = resolution.get_unsatisfying_offers();
    if offers.is_empty() && resolution.get_satisfying_offers().is_empty() {
        // A condition still holding a declared variable: say which variable
        // nothing named a value for.
        let pattern = Pattern::parse_lossy(behavior);
        if needed_by.is_some() && !pattern.is_ground() && !pattern.has_open() {
            let found = tracker.propose(behavior);
            let unnamed: Vec<String> = pattern
                .vars()
                .into_iter()
                .filter(|v| !found.names(v))
                .map(str::to_string)
                .collect();
            let complete = found.values.iter().any(|v| v.len() == pattern.vars().len());
            out.insert(if unnamed.is_empty() || complete {
                // Values were named; whatever keeps them is only reached
                // through a loop.
                Cause {
                    behavior: behavior.to_string(),
                    kind: CauseKind::Cycle,
                    needed_by,
                }
            } else {
                Cause {
                    behavior: behavior.to_string(),
                    kind: CauseKind::Unfound {
                        holds_for_any: unnamed
                            .iter()
                            .flat_map(|v| found.holding_for_any(v))
                            .cloned()
                            .collect(),
                        variables: unnamed,
                    },
                    needed_by,
                }
            });
            return;
        }
        out.insert(Cause {
            behavior: behavior.to_string(),
            kind: if tracker.has_provider(behavior) {
                CauseKind::Cycle
            } else {
                CauseKind::Unprovided
            },
            needed_by,
        });
        return;
    }
    for offer in offers {
        // A condition still waiting on a value a `{{?x}}` sibling was to
        // find is not itself a cause: the sibling that found nothing is.
        let conditions = offer.get_resolved_conditions();
        let opens = conditions
            .iter()
            .any(|c| Pattern::parse_lossy(c.get_behavior_name()).has_open());
        let waiting = |c: &Resolution| {
            opens
                && c.get_satisfying_offers().is_empty()
                && c.get_unsatisfying_offers().is_empty()
                && Pattern::parse_lossy(c.get_behavior_name()).needs_binding()
        };
        let other_cause = conditions
            .iter()
            .any(|c| !c.is_satisfied() && !waiting(c));
        for condition in conditions {
            if condition.is_satisfied() || (other_cause && waiting(condition)) {
                continue;
            }
            let site = CauseSite {
                agent: offer.get_agent_name().to_string(),
                // under an open goal, the concrete promise the offer keeps
                provide: offer.get_behavior().unwrap_or(behavior).to_string(),
            };
            leaf_causes(tracker, condition, Some(site), out);
        }
    }
}

/// Merge rows that are equal but for their agents, keeping the order of each
/// group's first row and the agents in the order met.
fn bundle<T: PartialEq>(rows: Vec<T>, agents: fn(&mut T) -> &mut Vec<String>) -> Vec<T> {
    let mut out: Vec<T> = vec![];
    for mut row in rows {
        let moved = std::mem::take(agents(&mut row));
        let at = out.iter_mut().position(|existing| {
            let held = std::mem::take(agents(existing));
            let same = *existing == row;
            *agents(existing) = held;
            same
        });
        match at {
            Some(at) => {
                let into = agents(&mut out[at]);
                for agent in moved {
                    if !into.contains(&agent) {
                        into.push(agent);
                    }
                }
            }
            None => {
                *agents(&mut row) = moved;
                out.push(row);
            }
        }
    }
    out
}

/// Resolutions remembered for the length of one query.
struct Resolver<'a> {
    tracker: &'a Tracker,
    cache: HashMap<String, Resolution>,
}

impl<'a> Resolver<'a> {
    fn new(tracker: &'a Tracker) -> Self {
        Resolver {
            tracker,
            cache: HashMap::new(),
        }
    }

    fn resolve(&mut self, behavior: &str) -> Resolution {
        self.cache
            .entry(behavior.to_string())
            .or_insert_with(|| self.tracker.resolve(behavior))
            .clone()
    }

    /// Whether each of a promise's conditions holds. Conditions that share a
    /// value are judged together, by the value that lets the most of them
    /// hold — all of them, where any value does.
    fn conditions(&mut self, promise: &Behavior) -> Vec<ConditionStatus> {
        let conditions = promise.get_conditions();
        let declared = promise.get_for_some();
        let shares = !declared.is_empty()
            || conditions
                .iter()
                .any(|c| Pattern::parse_lossy(c).needs_binding());
        let held: Vec<bool> = if shares {
            let readings = self.tracker.resolve_conditions(&conditions, declared);
            let count = |r: &Vec<Resolution>| r.iter().filter(|c| c.is_satisfied()).count();
            let mut best: Option<&Vec<Resolution>> = None;
            for reading in &readings {
                if best.map_or(true, |b| count(reading) > count(b)) {
                    best = Some(reading);
                }
            }
            match best {
                Some(reading) => reading.iter().map(|c| c.is_satisfied()).collect(),
                None => vec![false; conditions.len()],
            }
        } else {
            conditions
                .iter()
                .map(|c| self.resolve(c).is_satisfied())
                .collect()
        };
        conditions
            .into_iter()
            .zip(held)
            .map(|(behavior, held)| ConditionStatus { held, behavior })
            .collect()
    }
}

/// Every working agent's promises and wants, read once.
struct Index {
    promises: BTreeMap<String, Vec<Behavior>>,
    wants: BTreeMap<String, Vec<String>>,
}

impl Index {
    fn new(tracker: &Tracker) -> Self {
        let mut promises = BTreeMap::new();
        let mut wants = BTreeMap::new();
        for agent in tracker.get_working_agent_names() {
            promises.insert(
                agent.clone(),
                tracker.get_agent_promises(agent).unwrap_or_default(),
            );
            let mut w: Vec<String> = tracker.get_agent_wants(agent.clone()).into_iter().collect();
            w.sort();
            wants.insert(agent.clone(), w);
        }
        Index { promises, wants }
    }

    /// Every promise, by agent, in declaration order within an agent.
    fn promises(&self) -> impl Iterator<Item = (&String, &Behavior)> {
        self.promises
            .iter()
            .flat_map(|(agent, ps)| ps.iter().map(move |p| (agent, p)))
    }

    /// Does `agent` promise something that could keep `behavior`?
    fn provides(&self, agent: &str, behavior: &str) -> bool {
        self.promises
            .get(agent)
            .is_some_and(|ps| ps.iter().any(|p| p.match_goal(behavior).is_some()))
    }

    /// Who uses `behavior`, and how, by agent. A condition is reported once
    /// per promise carrying it.
    fn consumers(&self, behavior: &str) -> Vec<(String, ConsumerKind)> {
        let mut ret: Vec<(String, ConsumerKind)> = vec![];
        for (agent, wants) in &self.wants {
            // An open want is a want of everything it matches.
            if wants.iter().any(|w| {
                w == behavior || {
                    let want = Pattern::parse_lossy(w);
                    want.has_open() && want.match_ground(behavior).is_some()
                }
            }) {
                ret.push((agent.clone(), ConsumerKind::Want));
            }
        }
        for (agent, promise) in self.promises() {
            let needs = promise
                .get_condition_patterns()
                .iter()
                .any(|c| c.match_ground(behavior).is_some());
            let kind = ConsumerKind::Condition {
                provide: promise.get_name().clone(),
            };
            if needs && !ret.iter().any(|(a, k)| a == agent && k == &kind) {
                ret.push((agent.clone(), kind));
            }
        }
        ret
    }

    /// The agents that want `behavior` or need it in a condition.
    fn users(&self, behavior: &str) -> BTreeSet<String> {
        self.consumers(behavior)
            .into_iter()
            .map(|(a, _)| a)
            .collect()
    }

    /// `agent` promises `behavior` and is its only user.
    fn is_self_promise(&self, agent: &str, behavior: &str) -> bool {
        if !self.provides(agent, behavior) {
            return false;
        }
        let users = self.users(behavior);
        users.len() == 1 && users.contains(agent)
    }

    /// Every concrete behavior something asks for: the wants, the concrete
    /// conditions, and every condition resolution reaches from a want with
    /// its bindings filled in.
    fn needed_names(&self, resolver: &mut Resolver) -> BTreeSet<String> {
        let mut needed: BTreeSet<String> = BTreeSet::new();
        for (_, promise) in self.promises() {
            for condition in promise.get_condition_patterns() {
                if condition.is_ground() {
                    needed.insert(condition.source().clone());
                }
            }
        }
        let wants: BTreeSet<&String> = self.wants.values().flatten().collect();
        let mut walked: BTreeSet<String> = BTreeSet::new();
        for want in wants {
            collect_names(&resolver.resolve(want), &mut walked);
        }
        needed.extend(walked);
        needed
    }
}

/// Every behavior a resolution names, down through its conditions. A goal is
/// resolved the same way wherever it is reached, so one seen already is not
/// walked again.
fn collect_names(resolution: &Resolution, out: &mut BTreeSet<String>) {
    if !out.insert(resolution.get_behavior_name().to_string()) {
        return;
    }
    for offer in resolution
        .get_satisfying_offers()
        .iter()
        .chain(resolution.get_unsatisfying_offers())
    {
        // what an open goal was answered by is asked for too
        if let Some(kept) = offer.get_behavior() {
            out.insert(kept.to_string());
        }
        for condition in offer.get_resolved_conditions() {
            collect_names(condition, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, SuperAgent};

    const PLAIN: QueryOptions = QueryOptions {
        fold_self: false,
        bundle: false,
        fold_alternatives: false,
    };
    const SELF: QueryOptions = QueryOptions {
        fold_self: true,
        bundle: false,
        fold_alternatives: false,
    };
    const BUNDLE: QueryOptions = QueryOptions {
        fold_self: false,
        bundle: true,
        fold_alternatives: false,
    };
    const ALTERNATIVES: QueryOptions = QueryOptions {
        fold_self: false,
        bundle: false,
        fold_alternatives: true,
    };

    fn b(name: &str) -> Behavior {
        Behavior::build(name)
    }

    fn bc(name: &str, conditions: &[&str]) -> Behavior {
        Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
    }

    fn agent(name: &str, provides: Vec<Behavior>, wants: &[&str]) -> Agent {
        Agent::build(name)
            .with_provides(provides)
            .with_wants(wants.iter().map(|w| b(w)).collect())
    }

    fn tracker(agents: Vec<Agent>) -> Tracker {
        let mut t = Tracker::new();
        for a in agents {
            t.add_agent(a);
        }
        t
    }

    fn cond(behavior: &str, held: bool) -> ConditionStatus {
        ConditionStatus {
            behavior: behavior.to_string(),
            held,
        }
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn unused_names(t: &Tracker, options: QueryOptions) -> Vec<String> {
        unused(t, options).into_iter().map(|u| u.behavior).collect()
    }

    // --- Open variables ---------------------------------------------------

    #[test]
    fn a_promise_met_through_an_open_goal_is_used() {
        let t = tracker(vec![
            agent("op", vec![bc("configured", &["{{?doc}} shows usage"])], &["configured"]),
            agent("d1", vec![b("d1 shows usage")], &[]),
            agent("w", vec![], &["{{?who}} backs up"]),
            agent("bk", vec![b("bk backs up")], &[]),
            agent("idle", vec![b("nothing asks for this")], &[]),
        ]);
        assert_eq!(unused_names(&t, PLAIN), names(&["nothing asks for this"]));
        assert!(unsatisfied(&t, PLAIN).is_empty());

        // the open want shows among the behavior's consumers
        let used = behavior_use(&t, "bk backs up", PLAIN).unwrap();
        assert_eq!(
            used.consumers,
            vec![ConsumerRow {
                agents: names(&["w"]),
                kind: ConsumerKind::Want
            }]
        );
    }

    #[test]
    fn an_open_want_with_no_match_is_unsatisfied_under_its_own_text() {
        let t = tracker(vec![
            agent("w", vec![], &["{{?who}} backs up"]),
            agent("bk", vec![b("{{x}} backs up")], &[]),
        ]);
        assert_eq!(
            unsatisfied(&t, PLAIN),
            vec![UnsatisfiedRow {
                agents: names(&["w"]),
                behavior: "{{?who}} backs up".into(),
                causes: vec![Cause {
                    behavior: "{{?who}} backs up".into(),
                    kind: CauseKind::Unprovided,
                    needed_by: None,
                }],
            }]
        );
    }

    #[test]
    fn a_condition_waiting_on_a_sibling_is_not_a_cause() {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("o1")
                .with_provides(vec![Behavior::build("configured").with_conditions(vec![
                    String::from("will support {{?thing}}"),
                    String::from("{{thing}} does stuff"),
                ])])
                .with_wants(vec![Behavior::build("configured")]),
        );
        let rows = unsatisfied(&t, PLAIN);
        let causes: Vec<&str> = rows
            .iter()
            .flat_map(|r| r.causes.iter().map(|c| c.behavior.as_str()))
            .collect();
        assert_eq!(causes, vec!["will support {{?thing}}"]);
    }

    #[test]
    fn conditions_sharing_a_value_are_judged_together() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("o1").with_provides(vec![Behavior::build(
            "configured",
        )
        .with_conditions(vec![
            String::from("will support {{?thing}}"),
            String::from("{{thing}} does stuff"),
        ])]));
        t.add_agent(Agent::build("s1").with_provides(vec![
            Behavior::build("will support t1"),
            Behavior::build("will support t2"),
        ]));
        let held = |t: &Tracker| -> Vec<bool> {
            agent_provides(t, "o1", PLAIN).unwrap().provides[0]
                .conditions
                .iter()
                .map(|c| c.held)
                .collect()
        };
        assert_eq!(held(&t), vec![true, false]);
        t.add_agent(Agent::build("t2").with_provides(vec![Behavior::build("t2 does stuff")]));
        assert_eq!(held(&t), vec![true, true]);
    }

    #[test]
    fn a_failing_open_condition_names_the_promise_it_matched() {
        let t = tracker(vec![
            agent("op", vec![bc("configured", &["{{?doc}} shows usage"])], &["configured"]),
            agent("d1", vec![bc("d1 shows usage", &["d1 is approved"])], &[]),
        ]);
        let rows = unsatisfied(&t, PLAIN);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].causes,
            vec![Cause {
                behavior: "d1 is approved".into(),
                kind: CauseKind::Unprovided,
                needed_by: Some(CauseSite {
                    agent: "d1".into(),
                    provide: "d1 shows usage".into()
                }),
            }]
        );
    }

    // --- Agents -----------------------------------------------------------

    /// `op` serves an app on some `node` it runs on and that serves it.
    /// (The names end in literal text: matched two-sided, a condition that
    /// ends in a variable also meets any promise that starts with one, like
    /// `{{app}} is served`.)
    fn serving(with_host: bool) -> Tracker {
        let mut agents = vec![
            agent("w", vec![b("web is built")], &["web is served"]),
            agent(
                "op",
                vec![bc("{{app}} is served", &["{{app}} runs on {{node}} node", "{{node}} node serves {{app}}"])
                    .with_for_some(vec!["node".into()])],
                &[],
            ),
        ];
        if with_host {
            agents.push(agent(
                "host",
                vec![bc("{{app}} runs on {{node}} node", &["{{app}} is built"])],
                &[],
            ));
        }
        tracker(agents)
    }

    #[test]
    fn a_cause_names_the_variable_nothing_found_a_value_for() {
        let site = Some(CauseSite {
            agent: "op".into(),
            provide: "web is served".into(),
        });
        // host's promise holds for any node, and nothing names one
        assert_eq!(
            unsatisfied(&serving(true), PLAIN),
            vec![UnsatisfiedRow {
                agents: names(&["w"]),
                behavior: "web is served".into(),
                causes: vec![
                    Cause {
                        behavior: "web runs on {{node}} node".into(),
                        kind: CauseKind::Unfound {
                            variables: names(&["node"]),
                            holds_for_any: vec![crate::HoldsForAny {
                                variable: "node".into(),
                                agent: "host".into(),
                                promise: "{{app}} runs on {{node}} node".into(),
                            }],
                        },
                        needed_by: site.clone(),
                    },
                    Cause {
                        behavior: "{{node}} node serves web".into(),
                        kind: CauseKind::Unfound {
                            variables: names(&["node"]),
                            holds_for_any: vec![],
                        },
                        needed_by: site.clone(),
                    },
                ]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            }]
        );
        // nothing answers at all: the same, with no promise holding for any
        let rows = unsatisfied(&serving(false), PLAIN);
        assert!(rows[0].causes.iter().all(|c| matches!(
            &c.kind,
            CauseKind::Unfound { variables, holds_for_any }
                if variables == &names(&["node"]) && holds_for_any.is_empty()
        )));
    }

    #[test]
    fn a_gathered_row_nothing_answers_is_a_cause_naming_its_variable() {
        let t = tracker(vec![agent(
            "op",
            vec![bc("configured", &["{{doc}} shows usage"]).with_for_some(vec!["doc".into()])],
            &["configured"],
        )]);
        let rows = unsatisfied(&t, PLAIN);
        assert_eq!(
            rows[0].causes,
            vec![Cause {
                behavior: "{{doc}} shows usage".into(),
                kind: CauseKind::Unfound {
                    variables: names(&["doc"]),
                    holds_for_any: vec![],
                },
                needed_by: Some(CauseSite {
                    agent: "op".into(),
                    provide: "configured".into(),
                }),
            }]
        );
        // and it serializes with the variable
        let json = serde_json::to_string(&rows[0].causes[0].kind).unwrap();
        assert_eq!(json, r#"{"unfound":{"variables":["doc"],"holds_for_any":[]}}"#);
    }

    #[test]
    fn agent_provides_lists_conditions_held_and_broken() {
        let t = tracker(vec![
            agent("a", vec![b("z"), bc("y", &["held", "broken"]), b("x")], &[]),
            agent("p", vec![b("held")], &[]),
        ]);
        let got = agent_provides(&t, "a", PLAIN).unwrap();
        assert_eq!(got.agent, "a");
        assert_eq!(
            got.provides,
            vec![
                ProvideRow {
                    behavior: "x".into(),
                    conditions: vec![],
                    held: true
                },
                ProvideRow {
                    behavior: "y".into(),
                    conditions: vec![cond("held", true), cond("broken", false)],
                    held: false
                },
                ProvideRow {
                    behavior: "z".into(),
                    conditions: vec![],
                    held: true
                },
            ]
        );
        assert!(got.patterns.is_empty());
    }

    #[test]
    fn agent_provides_lists_patterns_apart() {
        let t = tracker(vec![agent(
            "a",
            vec![b("plain"), bc("db-{{n}}", &["disk-{{n}}"])],
            &[],
        )]);
        let got = agent_provides(&t, "a", PLAIN).unwrap();
        assert_eq!(got.provides.len(), 1);
        assert_eq!(got.provides[0].behavior, "plain");
        assert_eq!(
            got.patterns,
            vec![PatternProvideRow {
                pattern: "db-{{n}}".into(),
                conditions: names(&["disk-{{n}}"]),
            }]
        );
    }

    #[test]
    fn agent_provides_is_none_for_an_unknown_agent() {
        let t = tracker(vec![agent("a", vec![b("x")], &[])]);
        assert_eq!(agent_provides(&t, "nobody", PLAIN), None);
    }

    #[test]
    fn agent_provides_drops_self_promises_when_folding_self() {
        // a uses x alone (self-promise); y is used by a and c (not one);
        // z is unused (not a self-promise either).
        let t = tracker(vec![
            agent("a", vec![b("x"), b("y"), b("z")], &["x", "y"]),
            agent("c", vec![], &["y"]),
        ]);
        let all: Vec<String> = agent_provides(&t, "a", PLAIN)
            .unwrap()
            .provides
            .into_iter()
            .map(|r| r.behavior)
            .collect();
        assert_eq!(all, names(&["x", "y", "z"]));
        let folded: Vec<String> = agent_provides(&t, "a", SELF)
            .unwrap()
            .provides
            .into_iter()
            .map(|r| r.behavior)
            .collect();
        assert_eq!(folded, names(&["y", "z"]));
    }

    #[test]
    fn a_condition_on_its_own_promise_is_a_self_promise() {
        // a keeps x on y, which it keeps itself; c wants x.
        let t = tracker(vec![
            agent("a", vec![bc("x", &["y"]), b("y")], &[]),
            agent("c", vec![], &["x"]),
        ]);
        let folded: Vec<String> = agent_provides(&t, "a", SELF)
            .unwrap()
            .provides
            .into_iter()
            .map(|r| r.behavior)
            .collect();
        assert_eq!(folded, names(&["x"]));
    }

    // --- Behaviors --------------------------------------------------------

    #[test]
    fn behavior_use_lists_providers_and_consumers() {
        let t = tracker(vec![
            agent("p1", vec![b("x")], &[]),
            agent("p2", vec![bc("x", &["gone"])], &[]),
            agent("w", vec![], &["x"]),
            agent("n", vec![bc("y", &["x"]), bc("z", &["x"])], &[]),
        ]);
        let got = behavior_use(&t, "x", PLAIN).unwrap();
        assert!(got.satisfied);
        assert_eq!(
            got.providers,
            vec![
                ProviderRow {
                    agents: names(&["p1"]),
                    pattern: None,
                    conditions: vec![],
                    held: true
                },
                ProviderRow {
                    agents: names(&["p2"]),
                    pattern: None,
                    conditions: vec![cond("gone", false)],
                    held: false
                },
            ]
        );
        assert_eq!(
            got.consumers,
            vec![
                ConsumerRow {
                    agents: names(&["w"]),
                    kind: ConsumerKind::Want
                },
                ConsumerRow {
                    agents: names(&["n"]),
                    kind: ConsumerKind::Condition {
                        provide: "y".into()
                    }
                },
                ConsumerRow {
                    agents: names(&["n"]),
                    kind: ConsumerKind::Condition {
                        provide: "z".into()
                    }
                },
            ]
        );
    }

    #[test]
    fn behavior_use_resolves_a_pattern_providers_conditions_with_bindings() {
        let t = tracker(vec![
            agent("db", vec![bc("db-{{n}}", &["disk-{{n}}"])], &[]),
            agent("store", vec![b("disk-a")], &[]),
            agent("app", vec![], &["db-a", "db-b"]),
        ]);
        let a = behavior_use(&t, "db-a", PLAIN).unwrap();
        assert_eq!(
            a.providers,
            vec![ProviderRow {
                agents: names(&["db"]),
                pattern: Some("db-{{n}}".into()),
                conditions: vec![cond("disk-a", true)],
                held: true
            }]
        );
        let b_ = behavior_use(&t, "db-b", PLAIN).unwrap();
        assert!(!b_.satisfied);
        assert_eq!(b_.providers[0].conditions, vec![cond("disk-b", false)]);
        // A parameterized condition counts as needing what it could name.
        let disk = behavior_use(&t, "disk-a", PLAIN).unwrap();
        assert_eq!(
            disk.consumers,
            vec![ConsumerRow {
                agents: names(&["db"]),
                kind: ConsumerKind::Condition {
                    provide: "db-{{n}}".into()
                }
            }]
        );
    }

    #[test]
    fn behavior_use_is_none_for_an_unknown_behavior() {
        let t = tracker(vec![agent("a", vec![b("x")], &[])]);
        assert_eq!(behavior_use(&t, "nope", PLAIN), None);
        // Wanted but unprovided is still present.
        let t = tracker(vec![agent("a", vec![], &["x"])]);
        let got = behavior_use(&t, "x", PLAIN).unwrap();
        assert!(!got.satisfied);
        assert!(got.providers.is_empty());
        assert_eq!(got.consumers.len(), 1);
    }

    #[test]
    fn behavior_use_with_self_folding_hides_own_use() {
        // a provides and wants x; c wants x too, so x is not a self-promise.
        let t = tracker(vec![
            agent("a", vec![b("x")], &["x"]),
            agent("c", vec![], &["x"]),
        ]);
        let plain = behavior_use(&t, "x", PLAIN).unwrap();
        assert_eq!(plain.providers.len(), 1);
        assert_eq!(plain.consumers.len(), 2);
        let folded = behavior_use(&t, "x", SELF).unwrap();
        assert_eq!(folded.providers[0].agents, names(&["a"]));
        assert_eq!(
            folded.consumers,
            vec![ConsumerRow {
                agents: names(&["c"]),
                kind: ConsumerKind::Want
            }]
        );
    }

    #[test]
    fn behavior_use_with_self_folding_shows_nothing_for_a_self_promise() {
        let t = tracker(vec![agent("a", vec![b("x")], &["x"])]);
        let folded = behavior_use(&t, "x", SELF).unwrap();
        assert!(folded.providers.is_empty());
        assert!(folded.consumers.is_empty());
        let plain = behavior_use(&t, "x", PLAIN).unwrap();
        assert_eq!(plain.providers.len(), 1);
        assert_eq!(plain.consumers.len(), 1);
    }

    #[test]
    fn behavior_use_with_self_folding_hides_other_providers_of_a_self_promise() {
        // a is x's only user and keeps it itself; b's promise of x is to nobody.
        let t = tracker(vec![
            agent("a", vec![b("x")], &["x"]),
            agent("b", vec![b("x")], &[]),
        ]);
        let folded = behavior_use(&t, "x", SELF).unwrap();
        assert!(folded.providers.is_empty());
        assert!(folded.consumers.is_empty());
        assert_eq!(behavior_use(&t, "x", PLAIN).unwrap().providers.len(), 2);
    }

    #[test]
    fn agent_provides_folds_a_pattern_used_only_by_its_provider() {
        let t = tracker(vec![agent("a", vec![b("disk-{{d}}")], &["disk-a"])]);
        assert!(agent_provides(&t, "a", SELF).unwrap().patterns.is_empty());
        assert_eq!(agent_provides(&t, "a", PLAIN).unwrap().patterns.len(), 1);
        // Used by someone else too: kept.
        let t = tracker(vec![
            agent("a", vec![b("disk-{{d}}")], &["disk-a"]),
            agent("c", vec![], &["disk-b"]),
        ]);
        assert_eq!(agent_provides(&t, "a", SELF).unwrap().patterns.len(), 1);
    }

    #[test]
    fn behavior_use_bundles_rows_that_differ_only_by_agent() {
        let t = tracker(vec![
            agent("p1", vec![b("x")], &[]),
            agent("p2", vec![bc("x", &["c"])], &[]),
            agent("p3", vec![b("x")], &[]),
            agent("p4", vec![b("x")], &[]),
            agent("w1", vec![], &["x"]),
            agent("w2", vec![], &["x"]),
            agent("n1", vec![bc("y", &["x"])], &[]),
            agent("n2", vec![bc("y", &["x"])], &[]),
            agent("n3", vec![bc("z", &["x"])], &[]),
        ]);
        let got = behavior_use(&t, "x", BUNDLE).unwrap();
        assert_eq!(
            got.providers
                .iter()
                .map(|r| r.agents.clone())
                .collect::<Vec<_>>(),
            vec![names(&["p1", "p3", "p4"]), names(&["p2"])]
        );
        assert_eq!(
            got.consumers,
            vec![
                ConsumerRow {
                    agents: names(&["w1", "w2"]),
                    kind: ConsumerKind::Want
                },
                ConsumerRow {
                    agents: names(&["n1", "n2"]),
                    kind: ConsumerKind::Condition {
                        provide: "y".into()
                    }
                },
                ConsumerRow {
                    agents: names(&["n3"]),
                    kind: ConsumerKind::Condition {
                        provide: "z".into()
                    }
                },
            ]
        );
        // Unbundled, one agent per row.
        let plain = behavior_use(&t, "x", PLAIN).unwrap();
        assert_eq!(plain.providers.len(), 4);
        assert_eq!(plain.consumers.len(), 5);
        assert!(plain.consumers.iter().all(|r| r.agents.len() == 1));
    }

    // --- Unsatisfied ------------------------------------------------------

    #[test]
    fn unsatisfied_reports_a_missing_want() {
        let t = tracker(vec![
            agent("a", vec![], &["x", "ok"]),
            agent("p", vec![b("ok")], &[]),
        ]);
        assert_eq!(
            unsatisfied(&t, PLAIN),
            vec![UnsatisfiedRow {
                agents: names(&["a"]),
                behavior: "x".into(),
                causes: vec![Cause {
                    behavior: "x".into(),
                    kind: CauseKind::Unprovided,
                    needed_by: None
                }],
            }]
        );
    }

    #[test]
    fn unsatisfied_descends_to_the_first_missing_behavior() {
        // a wants x; p keeps x on c1; q keeps c1 on c2 (missing) and c3 (held).
        let t = tracker(vec![
            agent("a", vec![], &["x"]),
            agent("p", vec![bc("x", &["c1"])], &[]),
            agent("q", vec![bc("c1", &["c2", "c3"])], &[]),
            agent("r", vec![b("c3")], &[]),
        ]);
        assert_eq!(
            unsatisfied(&t, PLAIN),
            vec![UnsatisfiedRow {
                agents: names(&["a"]),
                behavior: "x".into(),
                causes: vec![Cause {
                    behavior: "c2".into(),
                    kind: CauseKind::Unprovided,
                    needed_by: Some(CauseSite {
                        agent: "q".into(),
                        provide: "c1".into()
                    }),
                }],
            }]
        );
    }

    #[test]
    fn unsatisfied_is_want_level_only() {
        // p's offer fails, but q still keeps x: nothing to report.
        let t = tracker(vec![
            agent("a", vec![], &["x"]),
            agent("p", vec![bc("x", &["gone"])], &[]),
            agent("q", vec![b("x")], &[]),
        ]);
        assert!(unsatisfied(&t, PLAIN).is_empty());
    }

    #[test]
    fn unsatisfied_lists_every_failing_offer_cause() {
        let t = tracker(vec![
            agent("a", vec![], &["x"]),
            agent("p", vec![bc("x", &["m1"])], &[]),
            agent("q", vec![bc("x", &["m2"])], &[]),
        ]);
        let rows = unsatisfied(&t, PLAIN);
        assert_eq!(
            rows[0]
                .causes
                .iter()
                .map(|c| c.behavior.clone())
                .collect::<Vec<_>>(),
            names(&["m1", "m2"])
        );
    }

    #[test]
    fn unsatisfied_marks_a_loop_as_a_cycle() {
        // x rests on y rests on x: both are promised, neither kept.
        let t = tracker(vec![
            agent("a", vec![], &["x"]),
            agent("p", vec![bc("x", &["y"])], &[]),
            agent("q", vec![bc("y", &["x"])], &[]),
        ]);
        let rows = unsatisfied(&t, PLAIN);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].causes,
            vec![Cause {
                behavior: "x".into(),
                kind: CauseKind::Cycle,
                needed_by: Some(CauseSite {
                    agent: "q".into(),
                    provide: "y".into()
                }),
            }]
        );
    }

    #[test]
    fn unsatisfied_bundles_agents_sharing_a_want() {
        let t = tracker(vec![
            agent("a1", vec![], &["x", "y"]),
            agent("a2", vec![], &["x"]),
            agent("a3", vec![], &["x"]),
        ]);
        let plain = unsatisfied(&t, PLAIN);
        assert_eq!(plain.len(), 4);
        let bundled = unsatisfied(&t, BUNDLE);
        assert_eq!(
            bundled
                .iter()
                .map(|r| (r.agents.clone(), r.behavior.clone()))
                .collect::<Vec<_>>(),
            vec![
                (names(&["a1", "a2", "a3"]), "x".to_string()),
                (names(&["a1"]), "y".to_string()),
            ]
        );
    }

    #[test]
    fn unsatisfied_ignores_self_folding() {
        let t = tracker(vec![
            agent("a", vec![b("x")], &["x", "gone"]),
            agent("c", vec![], &["gone"]),
        ]);
        assert_eq!(unsatisfied(&t, PLAIN), unsatisfied(&t, SELF));
        assert_eq!(unsatisfied(&t, PLAIN).len(), 2);
    }

    // --- Unused -----------------------------------------------------------

    #[test]
    fn unused_reports_what_nobody_wants_or_needs() {
        let t = tracker(vec![
            agent("p", vec![b("wanted"), b("conditioned"), b("spare")], &[]),
            agent("q", vec![bc("other", &["conditioned"]), b("spare")], &[]),
            agent("w", vec![], &["wanted"]),
        ]);
        assert_eq!(
            unused(&t, PLAIN),
            vec![
                UnusedBehavior {
                    behavior: "other".into(),
                    pattern: false,
                    providers: vec![UnusedProvider {
                        agents: names(&["q"]),
                        conditions: names(&["conditioned"]),
                    }],
                },
                UnusedBehavior {
                    behavior: "spare".into(),
                    pattern: false,
                    providers: vec![
                        UnusedProvider {
                            agents: names(&["p"]),
                            conditions: vec![],
                        },
                        UnusedProvider {
                            agents: names(&["q"]),
                            conditions: vec![],
                        },
                    ],
                },
            ]
        );
    }

    #[test]
    fn unused_counts_self_use_as_used_when_self_folding_is_off() {
        // a wants what it provides; a's own condition needs y.
        let t = tracker(vec![agent(
            "a",
            vec![b("x"), bc("z", &["y"]), b("y")],
            &["x"],
        )]);
        assert_eq!(unused_names(&t, PLAIN), names(&["z"]));
    }

    #[test]
    fn unused_does_not_report_self_promises_when_self_folding_is_on() {
        let t = tracker(vec![
            agent("a", vec![b("x"), bc("z", &["y"]), b("y")], &["x"]),
            agent("c", vec![b("w")], &[]),
        ]);
        assert_eq!(unused_names(&t, SELF), names(&["w", "z"]));
        assert_eq!(unused(&t, SELF), unused(&t, PLAIN));
    }

    #[test]
    fn unused_patterns_match_wants_and_reached_conditions() {
        let t = tracker(vec![
            agent("db", vec![bc("db-{{n}}", &["disk-{{n}}"])], &[]),
            agent("store", vec![b("disk-{{d}}")], &[]),
            agent("cache", vec![b("cache-{{n}}")], &[]),
            agent("app", vec![], &["db-a"]),
        ]);
        // db-{{n}} matches the want; disk-{{d}} matches disk-a, the condition
        // resolution reaches; cache-{{n}} matches nothing.
        assert_eq!(
            unused(&t, PLAIN),
            vec![UnusedBehavior {
                behavior: "cache-{{n}}".into(),
                pattern: true,
                providers: vec![UnusedProvider {
                    agents: names(&["cache"]),
                    conditions: vec![],
                }],
            }]
        );
    }

    #[test]
    fn unused_pattern_matches_a_ground_condition() {
        let t = tracker(vec![
            agent("n", vec![bc("y", &["port-80"])], &[]),
            agent("p", vec![b("port-{{n}}")], &[]),
        ]);
        assert_eq!(unused_names(&t, PLAIN), names(&["y"]));
    }

    #[test]
    fn unused_bundles_providers_that_differ_only_by_agent() {
        let t = tracker(vec![
            agent("p1", vec![b("x")], &[]),
            agent("p2", vec![b("x")], &[]),
            agent("p3", vec![bc("x", &["c"])], &[]),
        ]);
        let plain = unused(&t, PLAIN);
        assert_eq!(plain[0].providers.len(), 3);
        let bundled = unused(&t, BUNDLE);
        assert_eq!(
            bundled[0].providers,
            vec![
                UnusedProvider {
                    agents: names(&["p1", "p2"]),
                    conditions: vec![],
                },
                UnusedProvider {
                    agents: names(&["p3"]),
                    conditions: names(&["c"]),
                },
            ]
        );
    }

    // --- SuperAgents ------------------------------------------------------

    /// a1 keeps b1 on b2, which a2 promises; both sit in sa1, and c wants b1.
    fn collective() -> Tracker {
        let mut t = tracker(vec![
            agent("a1", vec![bc("b1", &["b2"])], &[]),
            agent("a2", vec![b("b2")], &[]),
            agent("c", vec![], &["b1"]),
        ]);
        t.add_superagent(
            SuperAgent::new("sa1".to_string())
                .with_agent("a1")
                .with_agent("a2"),
        );
        t
    }

    #[test]
    fn folded_collectives_answer_as_one_agent() {
        let t = collective();
        assert_eq!(agent_provides(&t, "a1", PLAIN), None);
        let sa = agent_provides(&t, "sa1", PLAIN).unwrap();
        // The internal condition is reduced away.
        assert_eq!(
            sa.provides,
            vec![
                ProvideRow {
                    behavior: "b1".into(),
                    conditions: vec![],
                    held: true
                },
                ProvideRow {
                    behavior: "b2".into(),
                    conditions: vec![],
                    held: true
                },
            ]
        );
        let b1 = behavior_use(&t, "b1", PLAIN).unwrap();
        assert_eq!(b1.providers[0].agents, names(&["sa1"]));
        // Folded, nothing outside the collective needs b2.
        assert_eq!(unused_names(&t, PLAIN), names(&["b2"]));
    }

    #[test]
    fn unfolded_collectives_answer_member_by_member() {
        let mut t = collective();
        t.set_fold_collectives(false);
        assert_eq!(agent_provides(&t, "sa1", PLAIN), None);
        let a1 = agent_provides(&t, "a1", PLAIN).unwrap();
        assert_eq!(a1.provides[0].conditions, vec![cond("b2", true)]);
        let b2 = behavior_use(&t, "b2", PLAIN).unwrap();
        assert_eq!(
            b2.consumers,
            vec![ConsumerRow {
                agents: names(&["a1"]),
                kind: ConsumerKind::Condition {
                    provide: "b1".into()
                }
            }]
        );
        assert!(unused_names(&t, PLAIN).is_empty());
    }

    #[test]
    fn results_serialize() {
        let t = collective();
        let got = behavior_use(&t, "b1", PLAIN).unwrap();
        let json = serde_json::to_string(&got).unwrap();
        assert!(json.contains("\"kind\":\"want\""), "{}", json);
        let back: BehaviorUse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, got);
    }

    // --- Fold: Unneeded alternatives ---------------------------------------

    fn alternatives() -> Tracker {
        tracker(vec![
            agent("x", vec![], &["b1", "b2", "top"]),
            agent("good", vec![b("b1")], &[]),
            // b1 is kept by good, so this offer of it is not needed; b2 it
            // keeps; top nobody keeps
            agent(
                "alt",
                vec![bc("b1", &["c", "d"]), bc("b2", &["c"]), bc("top", &["d"])],
                &[],
            ),
            agent("other", vec![bc("top", &["c", "d"])], &[]),
            agent("cprov", vec![b("c")], &[]),
        ])
    }

    #[test]
    fn behavior_use_drops_providers_not_held_when_another_is() {
        let t = alternatives();
        let plain = behavior_use(&t, "b1", PLAIN).unwrap();
        assert_eq!(plain.providers.len(), 2);

        let folded = behavior_use(&t, "b1", ALTERNATIVES).unwrap();
        assert_eq!(
            folded.providers,
            vec![ProviderRow {
                agents: names(&["good"]),
                pattern: None,
                conditions: vec![],
                held: true,
            }]
        );
        assert!(folded.satisfied);
        assert_eq!(folded.consumers, plain.consumers);
    }

    #[test]
    fn behavior_use_keeps_every_provider_when_none_is_held() {
        let t = alternatives();
        let plain = behavior_use(&t, "top", PLAIN).unwrap();
        assert_eq!(plain.providers.len(), 2);
        assert!(!plain.satisfied);
        assert_eq!(behavior_use(&t, "top", ALTERNATIVES).unwrap(), plain);
        // and every one that is held
        assert_eq!(
            behavior_use(&t, "b2", ALTERNATIVES).unwrap(),
            behavior_use(&t, "b2", PLAIN).unwrap()
        );
    }

    #[test]
    fn behavior_use_folds_alternatives_before_bundling() {
        let t = tracker(vec![
            agent("x", vec![], &["b1"]),
            agent("g1", vec![b("b1")], &[]),
            agent("g2", vec![b("b1")], &[]),
            agent("n1", vec![bc("b1", &["d"])], &[]),
            agent("n2", vec![bc("b1", &["d"])], &[]),
        ]);
        let both = QueryOptions {
            bundle: true,
            ..ALTERNATIVES
        };
        let got = behavior_use(&t, "b1", both).unwrap();
        assert_eq!(got.providers.len(), 1);
        assert_eq!(got.providers[0].agents, names(&["g1", "g2"]));
    }

    #[test]
    fn agent_provides_drops_a_row_only_when_the_behavior_is_kept_elsewhere() {
        let t = alternatives();
        let plain = agent_provides(&t, "alt", PLAIN).unwrap();
        assert_eq!(
            plain
                .provides
                .iter()
                .map(|r| (r.behavior.as_str(), r.held))
                .collect::<Vec<_>>(),
            vec![("b1", false), ("b2", true), ("top", false)]
        );

        let folded = agent_provides(&t, "alt", ALTERNATIVES).unwrap();
        // b1 is kept by good: gone. b2 is held: stays. top nobody keeps:
        // stays, broken as it is.
        assert_eq!(
            folded.provides,
            vec![
                ProvideRow {
                    behavior: "b2".to_string(),
                    conditions: vec![cond("c", true)],
                    held: true,
                },
                ProvideRow {
                    behavior: "top".to_string(),
                    conditions: vec![cond("d", false)],
                    held: false,
                },
            ]
        );
        // an agent with nothing unneeded reads the same
        assert_eq!(
            agent_provides(&t, "good", ALTERNATIVES),
            agent_provides(&t, "good", PLAIN)
        );
        assert_eq!(
            agent_provides(&t, "other", ALTERNATIVES),
            agent_provides(&t, "other", PLAIN)
        );
    }

    #[test]
    fn agent_provides_drops_its_own_broken_row_beside_its_own_held_one() {
        // promised outright and under a condition nobody keeps: the agent
        // itself keeps the behavior, so its broken row is the alternative
        let t = tracker(vec![
            agent("x", vec![], &["b1"]),
            agent("a", vec![b("b1"), bc("b1", &["d"])], &[]),
        ]);
        assert_eq!(agent_provides(&t, "a", PLAIN).unwrap().provides.len(), 2);
        let folded = agent_provides(&t, "a", ALTERNATIVES).unwrap();
        assert_eq!(folded.provides.len(), 1);
        assert!(folded.provides[0].held);
    }

    #[test]
    fn unsatisfied_and_unused_ignore_alternative_folding() {
        let mut agents_tracker = alternatives();
        agents_tracker.add_agent(agent("idle", vec![b("unasked"), bc("unasked", &["d"])], &[]));
        let t = agents_tracker;
        assert_eq!(unsatisfied(&t, ALTERNATIVES), unsatisfied(&t, PLAIN));
        assert_eq!(unsatisfied(&t, PLAIN).len(), 1);
        assert_eq!(unused(&t, ALTERNATIVES), unused(&t, PLAIN));
        assert_eq!(unused_names(&t, PLAIN), names(&["unasked"]));
        assert_eq!(unused(&t, PLAIN)[0].providers.len(), 2);
        let bundled = QueryOptions {
            bundle: true,
            ..ALTERNATIVES
        };
        assert_eq!(unsatisfied(&t, bundled), unsatisfied(&t, BUNDLE));
        assert_eq!(unused(&t, bundled), unused(&t, BUNDLE));
    }

    #[test]
    fn options_without_the_field_read_as_off() {
        let old: QueryOptions =
            serde_json::from_str(r#"{"fold_self":true,"bundle":false}"#).unwrap();
        assert_eq!(old, SELF);
    }
}
