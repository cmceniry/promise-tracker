//! Where substituted values sit in the behavior names a contract resolves to,
//! so a view can draw them wrapped in `<>`: `<A1> reads R1`.
//!
//! This is the one place the "Mark: Bound values" display option reads from.
//! The graph, trail and inventory rows keep plain names — a name is a
//! behavior's identity, and the key every lookup and selection uses — and a
//! view that is asked to mark looks the label up here by the same plain name.
//! Nothing a [`Marks`] returns is ever a behavior name.
//!
//! A value counts as substituted when it took the place of a variable:
//!
//! - `{{!self}}`, replaced by the declaring agent's name;
//! - an Instance `bindings:` value;
//! - what a parameterized promise name took from the goal it keeps
//!   (`{{agent}} reads R1` keeping `A1 reads R1`), and the same value where
//!   it went on into the promise's conditions;
//! - a value one condition found with `{{?x}}` and a sibling used as `{{x}}`;
//! - the part of a promised behavior an open goal's `{{?x}}` matched.
//!
//! The same text can be reached with different parts substituted — one agent
//! writes `{{!self}} is up`, another spells the name out — so marks are kept
//! by who is looking: the agent that needs the behavior, and the agent that
//! promises it. A line between the two shows both.

use crate::components::{mark_spans, merge_spans, Pattern};
use crate::resolve::{Resolution, Spans};
use crate::{as_reached_by, Tracker};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The substituted parts of every behavior name in a tracker, by who needs
/// it and who promises it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// (agent, name) — the name as that agent needs it: its want, or a
    /// condition on one of its promises as resolution reached it. For an
    /// open need this holds both the pattern and each behavior it matched.
    need: BTreeMap<(String, String), Spans>,
    /// (agent, name) — the name as that agent promises it.
    offer: BTreeMap<(String, String), Spans>,
    /// `need` over every agent.
    needed: BTreeMap<String, Spans>,
    /// `offer` over every agent.
    offered: BTreeMap<String, Spans>,
    agents: BTreeSet<String>,
}

/// What [`Marks::walk`] has already been through.
#[derive(Default)]
struct Walked {
    /// Goals whose promises' conditions have been recorded.
    goals: HashSet<String>,
    /// A goal, with the offers that answered it: what hangs under those has
    /// been recorded.
    answers: HashSet<(String, Vec<String>)>,
}

impl Marks {
    /// Read every declaration and every resolution in the tracker.
    pub fn new(tracker: &Tracker) -> Marks {
        let mut marks = Marks::default();
        let mut agent_names: Vec<&String> = tracker.working_agents.keys().collect();
        agent_names.sort();

        // What each agent wrote: `{{!self}}` and Instance bindings, fixed
        // when the working agents were built.
        for agent in &agent_names {
            marks.agents.insert(agent.to_string());
            for variant in &tracker.working_agents[*agent] {
                for promise in variant.provides() {
                    let name = promise.get_name_pattern();
                    marks.add_offer(agent, name.source(), name.bound());
                    for condition in promise.get_condition_patterns() {
                        marks.add_need(agent, condition.source(), condition.bound());
                    }
                }
                for want in variant.wants() {
                    let name = want.get_name_pattern();
                    marks.add_need(agent, name.source(), name.bound());
                }
            }
        }

        // What resolution filled in on the way down from every want, and
        // from every behavior that is named outright.
        let mut settled: HashMap<String, Resolution> = HashMap::new();
        let mut walked = Walked::default();
        for agent in &agent_names {
            let mut wants: Vec<Pattern> = tracker.working_agents[*agent]
                .iter()
                .flat_map(|variant| variant.wants().iter().map(|w| w.get_name_pattern().clone()))
                .collect();
            wants.sort();
            for want in wants {
                if want.needs_binding() {
                    continue;
                }
                let (resolved, _) = tracker.resolve_goal(want.source(), &mut vec![], &mut settled);
                marks.walk(
                    tracker,
                    Some(agent),
                    &as_reached_by(resolved, &want),
                    &mut walked,
                );
            }
        }
        let mut behaviors: Vec<String> = tracker.get_working_behaviors().into_iter().collect();
        behaviors.sort();
        for behavior in behaviors {
            if Pattern::parse_lossy(&behavior).needs_binding() {
                continue;
            }
            let (resolved, _) = tracker.resolve_goal(&behavior, &mut vec![], &mut settled);
            marks.walk(tracker, None, &resolved, &mut walked);
        }
        marks
    }

    fn add_need(&mut self, agent: &str, name: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .need
            .entry((agent.to_string(), name.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
        let held = self.needed.entry(name.to_string()).or_default();
        *held = merge_spans(held, spans);
    }

    fn add_offer(&mut self, agent: &str, name: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .offer
            .entry((agent.to_string(), name.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
        let held = self.offered.entry(name.to_string()).or_default();
        *held = merge_spans(held, spans);
    }

    /// Record a resolution `holder` needs, and everything under it.
    fn walk(
        &mut self,
        tracker: &Tracker,
        holder: Option<&str>,
        resolution: &Resolution,
        walked: &mut Walked,
    ) {
        let name = resolution.get_behavior_name();
        if let Some(holder) = holder {
            self.add_need(holder, name, resolution.get_bound());
        }
        // The conditions of every promise that could keep this goal, with
        // only the goal's values filled in. Resolution reads on from there —
        // a sibling's `{{?x}}` supplies the rest — but the inventory lists
        // a promise's conditions at this stage, so it is recorded too.
        if walked.goals.insert(name.to_string()) {
            for (agent, variants) in &tracker.working_agents {
                for variant in variants {
                    for (promise, bindings) in variant.get_matching_provides(name) {
                        for condition in promise.get_condition_patterns() {
                            let condition = condition.substitute(&bindings);
                            self.add_need(agent, condition.source(), condition.bound());
                        }
                    }
                }
            }
        }
        let offers = || {
            resolution
                .get_satisfying_offers()
                .iter()
                .chain(resolution.get_unsatisfying_offers())
        };
        let mut kept_names: Vec<String> = vec![];
        for offer in offers() {
            let kept = offer.get_behavior().unwrap_or(name);
            self.add_offer(offer.get_agent_name(), kept, offer.get_bound());
            if let (Some(holder), Some(kept)) = (holder, offer.get_behavior()) {
                self.add_need(holder, kept, offer.get_matched());
            }
            kept_names.push(format!("{}\u{0}{}", offer.get_agent_name(), kept));
        }
        // What hangs under the offers depends only on the goal and on which
        // matches were kept, not on who asked: once is enough.
        if !walked.answers.insert((name.to_string(), kept_names)) {
            return;
        }
        for offer in offers() {
            for condition in offer.get_resolved_conditions() {
                self.walk(tracker, Some(offer.get_agent_name()), condition, walked);
            }
        }
    }

    fn spans<'a>(
        map: &'a BTreeMap<(String, String), Spans>,
        agent: &str,
        name: &str,
    ) -> &'a [(usize, usize)] {
        map.get(&(agent.to_string(), name.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn all<'a>(map: &'a BTreeMap<String, Spans>, name: &str) -> &'a [(usize, usize)] {
        map.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Nothing in the tracker was substituted.
    pub fn is_empty(&self) -> bool {
        self.need.is_empty() && self.offer.is_empty()
    }

    /// `name` as `agent` needs it: its want, or a condition on one of its
    /// promises. The plain name when the agent substituted nothing.
    pub fn need(&self, agent: &str, name: &str) -> String {
        mark_spans(name, Self::spans(&self.need, agent, name))
    }

    /// [`Marks::need`] for a row standing for several agents: what any of
    /// them substituted.
    pub fn need_by<S: AsRef<str>>(&self, agents: &[S], name: &str) -> String {
        let spans = agents.iter().fold(vec![], |acc, agent| {
            merge_spans(&acc, Self::spans(&self.need, agent.as_ref(), name))
        });
        mark_spans(name, &spans)
    }

    /// `name` as `agent` promises it: a concrete behavior, or a
    /// parameterized promise as written.
    pub fn promise(&self, agent: &str, name: &str) -> String {
        mark_spans(name, Self::spans(&self.offer, agent, name))
    }

    /// [`Marks::promise`] for a row standing for several agents.
    pub fn promise_by<S: AsRef<str>>(&self, agents: &[S], name: &str) -> String {
        let spans = agents.iter().fold(vec![], |acc, agent| {
            merge_spans(&acc, Self::spans(&self.offer, agent.as_ref(), name))
        });
        mark_spans(name, &spans)
    }

    /// `name` as `agent` promises it and as everyone leaning on it needs it:
    /// the label for one promise shown once however many reach it.
    pub fn kept(&self, agent: &str, name: &str) -> String {
        mark_spans(
            name,
            &merge_spans(
                Self::spans(&self.offer, agent, name),
                Self::all(&self.needed, name),
            ),
        )
    }

    /// `name` on the line from `consumer` to `provider`: what either end
    /// substituted. An end that is not an agent of the tracker — a scratch
    /// group, a missing behavior's ghost — stands for every agent.
    pub fn edge(&self, consumer: &str, provider: &str, name: &str) -> String {
        let need = if self.agents.contains(consumer) {
            Self::spans(&self.need, consumer, name)
        } else {
            Self::all(&self.needed, name)
        };
        let offer = if self.agents.contains(provider) {
            Self::spans(&self.offer, provider, name)
        } else {
            Self::all(&self.offered, name)
        };
        mark_spans(name, &merge_spans(need, offer))
    }

    /// `name` as anyone needs it, for a behavior nobody promises.
    pub fn needed(&self, name: &str) -> String {
        mark_spans(name, Self::all(&self.needed, name))
    }

    /// `name` with everything any agent substituted, for a listing that
    /// belongs to no one agent.
    pub fn any(&self, name: &str) -> String {
        mark_spans(
            name,
            &merge_spans(
                Self::all(&self.needed, name),
                Self::all(&self.offered, name),
            ),
        )
    }
}

/// A tracker from contract YAML, for tests here and in the renderers.
#[cfg(test)]
pub(crate) fn tracker(yaml: &str) -> Tracker {
    let mut t = Tracker::new();
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        let item: crate::components::Item = serde::Deserialize::deserialize(doc).unwrap();
        t.add_item(item);
    }
    t
}

/// The IAM chain: a goal binds a parameterized promise, one condition
/// finds a value with `{{?approle}}` and its sibling uses it.
#[cfg(test)]
pub(crate) const IAM: &str = r#"
kind: Agent
name: AWSSM
provides:
- name: Read Secret Contents {{secret_name}} by {{application}}
  conditions:
    - "{{application}} has IAM role {{?approle}}"
    - "IAM access granted for reading Secret {{secret_name}} to {{approle}}"
---
kind: Agent
name: infrateam
provides:
- name: "IAM access granted for reading Secret {{secret_name}} to {{approle}}"
  conditions:
  - "Design approval access to Secret {{secret_name}} for {{?application}}"
---
kind: Agent
name: manager
provides:
- name: "Design approval access to Secret appsecret for appname"
---
kind: Agent
name: appname
wants:
- name: "Read Secret Contents appsecret by {{!self}}"
provides:
- name: "{{!self}} has IAM role approle"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const WANT: &str = "Read Secret Contents appsecret by appname";
    const ROLE: &str = "appname has IAM role approle";
    const OPEN_ROLE: &str = "appname has IAM role {{?approle}}";
    const GRANT: &str = "IAM access granted for reading Secret appsecret to approle";
    const APPROVAL: &str = "Design approval access to Secret appsecret for appname";
    const OPEN_APPROVAL: &str = "Design approval access to Secret appsecret for {{?application}}";

    #[test]
    fn a_want_is_marked_as_its_agent_wrote_it() {
        let m = Marks::new(&tracker(IAM));
        assert_eq!(
            m.need("appname", WANT),
            "Read Secret Contents appsecret by <appname>"
        );
        // Nobody else wants it.
        assert_eq!(m.need("manager", WANT), WANT);
    }

    #[test]
    fn a_parameterized_promise_marks_what_the_goal_bound() {
        let m = Marks::new(&tracker(IAM));
        assert_eq!(
            m.promise("AWSSM", WANT),
            "Read Secret Contents <appsecret> by <appname>"
        );
        assert_eq!(
            m.edge("appname", "AWSSM", WANT),
            "Read Secret Contents <appsecret> by <appname>"
        );
    }

    #[test]
    fn a_goal_bound_condition_is_marked() {
        let m = Marks::new(&tracker(IAM));
        // The value came down from the goal ...
        assert_eq!(
            m.need("AWSSM", OPEN_ROLE),
            "<appname> has IAM role {{?approle}}"
        );
        // ... and sideways from the sibling's `{{?approle}}`.
        assert_eq!(
            m.need("AWSSM", GRANT),
            "IAM access granted for reading Secret <appsecret> to <approle>"
        );
        assert_eq!(
            m.need("infrateam", OPEN_APPROVAL),
            "Design approval access to Secret <appsecret> for {{?application}}"
        );
    }

    #[test]
    fn a_condition_waiting_on_a_sibling_is_marked_with_what_the_goal_bound() {
        let m = Marks::new(&tracker(IAM));
        // As the inventory lists AWSSM's promise for this goal: the goal's
        // values in, the sibling's not yet.
        assert_eq!(
            m.need(
                "AWSSM",
                "IAM access granted for reading Secret appsecret to {{approle}}"
            ),
            "IAM access granted for reading Secret <appsecret> to {{approle}}"
        );
    }

    #[test]
    fn what_an_open_variable_matched_is_marked_on_the_behavior_kept() {
        let m = Marks::new(&tracker(IAM));
        // As AWSSM needs it: the goal's own value, and the open match.
        assert_eq!(m.need("AWSSM", ROLE), "<appname> has IAM role <approle>");
        // As appname promises it: only its own name.
        assert_eq!(m.promise("appname", ROLE), "<appname> has IAM role approle");
        assert_eq!(
            m.edge("AWSSM", "appname", ROLE),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(m.kept("appname", ROLE), "<appname> has IAM role <approle>");
        // manager wrote its promise out in full; infrateam's condition is
        // what filled it in.
        assert_eq!(m.promise("manager", APPROVAL), APPROVAL);
        assert_eq!(
            m.edge("infrateam", "manager", APPROVAL),
            "Design approval access to Secret <appsecret> for <appname>"
        );
    }

    #[test]
    fn an_end_that_is_no_agent_stands_for_everyone() {
        let m = Marks::new(&tracker(IAM));
        assert_eq!(
            m.edge("group:mine", "appname", ROLE),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(
            m.edge("AWSSM", "group:mine", ROLE),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(m.any(ROLE), "<appname> has IAM role <approle>");
        assert_eq!(m.needed(OPEN_ROLE), "<appname> has IAM role {{?approle}}");
    }

    #[test]
    fn rows_for_several_agents_show_what_any_of_them_substituted() {
        let m = Marks::new(&tracker(
            r#"
kind: Agent
name: a1
wants:
- name: "{{!self}} is up"
- name: a2 is up
---
kind: Agent
name: a2
wants:
- name: a1 is up
provides:
- name: "{{!self}} is up"
"#,
        ));
        assert_eq!(m.need("a1", "a1 is up"), "<a1> is up");
        assert_eq!(m.need("a2", "a1 is up"), "a1 is up");
        assert_eq!(m.need_by(&["a1", "a2"], "a1 is up"), "<a1> is up");
        assert_eq!(m.promise_by(&["a1", "a2"], "a2 is up"), "<a2> is up");
        assert_eq!(m.need_by::<&str>(&[], "a1 is up"), "a1 is up");
    }

    #[test]
    fn a_contract_with_no_variables_has_no_marks() {
        let m = Marks::new(&tracker(
            r#"
kind: Agent
name: a1
wants:
- name: b1
---
kind: Agent
name: a2
provides:
- name: b1
  conditions: [b2]
"#,
        ));
        assert!(m.is_empty());
        assert_eq!(m.edge("a1", "a2", "b1"), "b1");
        assert_eq!(m.any("b2"), "b2");
    }

    #[test]
    fn an_unprovided_parameterized_promise_keeps_its_declared_marks() {
        let m = Marks::new(&tracker(
            r#"
kind: Agent
name: S1
provides:
- name: "{{agent}} reads R1"
  conditions:
    - "{{!self}} configured for {{agent}}"
"#,
        ));
        assert_eq!(m.promise("S1", "{{agent}} reads R1"), "{{agent}} reads R1");
        assert_eq!(
            m.need("S1", "S1 configured for {{agent}}"),
            "<S1> configured for {{agent}}"
        );
    }
}
