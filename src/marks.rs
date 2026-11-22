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
//!
//! One agent can also reach the same text two ways itself: one promise names
//! a condition outright, another finds it with `{{?x}}`. So there are two
//! grains. The Overview's lines and nodes stand for an agent and a name, and
//! read the coarse one ([`Marks::need`], [`Marks::promise`], [`Marks::edge`]).
//! A listing whose row is one want, one promise or one condition of one
//! promise reads the fine one ([`Marks::want`], [`Marks::made`],
//! [`Marks::condition`]), so a row shows only what its own declaration
//! substituted. A trail reads neither: it carries its own spans
//! ([`crate::trace::Trail::labels`]).

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
    /// (agent, want) — a want as that agent wrote it, and nothing its
    /// promises' conditions say about the same text.
    want: BTreeMap<(String, String), Spans>,
    /// (agent, promise kept, condition) — a condition of one promise, under
    /// the behavior that promise keeps: as written, with a goal's values
    /// filled in, as resolution reached it, and each behavior an open
    /// condition matched.
    condition: BTreeMap<(String, String, String), Spans>,
    /// (agent, promise as written, behavior kept) — what one promise names
    /// through a variable in a behavior it keeps. As written and kept are
    /// the same text for the declaration itself.
    made: BTreeMap<(String, String, String), Spans>,
    agents: BTreeSet<String>,
}

/// Who holds the need [`Marks::walk`] is recording.
#[derive(Clone, Copy)]
enum Holder<'a> {
    /// A want of `agent`.
    Want { agent: &'a str },
    /// A condition on the promise by which `agent` keeps `promise`.
    Condition { agent: &'a str, promise: &'a str },
}

impl<'a> Holder<'a> {
    fn agent(&self) -> &'a str {
        match self {
            Holder::Want { agent } | Holder::Condition { agent, .. } => agent,
        }
    }
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
                    marks.add_made(agent, name.source(), name.source(), name.bound());
                    for condition in promise.get_condition_patterns() {
                        marks.add_need(agent, condition.source(), condition.bound());
                        marks.add_condition(
                            agent,
                            name.source(),
                            condition.source(),
                            condition.bound(),
                        );
                    }
                }
                for want in variant.wants() {
                    let name = want.get_name_pattern();
                    marks.add_need(agent, name.source(), name.bound());
                    marks.add_want(agent, name.source(), name.bound());
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
                    Some(Holder::Want { agent }),
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

    fn add_want(&mut self, agent: &str, name: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .want
            .entry((agent.to_string(), name.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
    }

    fn add_condition(&mut self, agent: &str, promise: &str, name: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .condition
            .entry((agent.to_string(), promise.to_string(), name.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
    }

    fn add_made(&mut self, agent: &str, written: &str, kept: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .made
            .entry((agent.to_string(), written.to_string(), kept.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
    }

    /// `name`, under `holder`, with `spans` substituted.
    fn add_held(&mut self, holder: Holder, name: &str, spans: &[(usize, usize)]) {
        self.add_need(holder.agent(), name, spans);
        match holder {
            // What an open want matched is not a want of its own: only the
            // want's text is kept by want.
            Holder::Want { agent } => self.add_want(agent, name, spans),
            Holder::Condition { agent, promise } => {
                self.add_condition(agent, promise, name, spans)
            }
        }
    }

    /// Record a resolution `holder` needs, and everything under it.
    fn walk(
        &mut self,
        tracker: &Tracker,
        holder: Option<Holder>,
        resolution: &Resolution,
        walked: &mut Walked,
    ) {
        let name = resolution.get_behavior_name();
        if let Some(holder) = holder {
            self.add_held(holder, name, resolution.get_bound());
        }
        // The conditions of every promise that could keep this goal, with
        // only the goal's values filled in. Resolution reads on from there —
        // a sibling's `{{?x}}` supplies the rest — but the inventory lists
        // a promise's conditions at this stage, so it is recorded too.
        if walked.goals.insert(name.to_string()) {
            for (agent, variants) in &tracker.working_agents {
                for variant in variants {
                    for (promise, bindings) in variant.get_matching_provides(name) {
                        // What this one promise names through a variable in
                        // the goal. A promise that names it outright has
                        // that from its declaration already.
                        let written = promise.get_name_pattern();
                        if !written.is_ground() {
                            let kept = written.substitute(&bindings);
                            self.add_made(agent, written.source(), name, kept.bound());
                        }
                        for condition in promise.get_condition_patterns() {
                            let condition = condition.substitute(&bindings);
                            self.add_need(agent, condition.source(), condition.bound());
                            self.add_condition(
                                agent,
                                name,
                                condition.source(),
                                condition.bound(),
                            );
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
            match (holder, offer.get_behavior()) {
                (Some(Holder::Want { agent }), Some(kept)) => {
                    self.add_need(agent, kept, offer.get_matched());
                }
                (Some(holder @ Holder::Condition { .. }), Some(kept)) => {
                    self.add_held(holder, kept, offer.get_matched());
                }
                _ => {}
            }
            kept_names.push(format!("{}\u{0}{}", offer.get_agent_name(), kept));
        }
        // What hangs under the offers depends only on the goal and on which
        // matches were kept, not on who asked: once is enough.
        if !walked.answers.insert((name.to_string(), kept_names)) {
            return;
        }
        for offer in offers() {
            // Its conditions are conditions of the promise keeping what the
            // offer keeps.
            let holder = Holder::Condition {
                agent: offer.get_agent_name(),
                promise: offer.get_behavior().unwrap_or(name),
            };
            for condition in offer.get_resolved_conditions() {
                self.walk(tracker, Some(holder), condition, walked);
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

    fn spans3<'a>(
        map: &'a BTreeMap<(String, String, String), Spans>,
        agent: &str,
        of: &str,
        name: &str,
    ) -> &'a [(usize, usize)] {
        map.get(&(agent.to_string(), of.to_string(), name.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Nothing in the tracker was substituted.
    pub fn is_empty(&self) -> bool {
        self.need.is_empty()
            && self.offer.is_empty()
            && self.want.is_empty()
            && self.condition.is_empty()
            && self.made.is_empty()
    }

    /// `name` as `agent` wants it: what the agent's own `{{!self}}` or
    /// Instance binding filled in. Nothing from a condition of the agent's
    /// that reaches the same text. The plain name when the agent wrote the
    /// want out, or does not want it.
    pub fn want(&self, agent: &str, name: &str) -> String {
        mark_spans(name, Self::spans(&self.want, agent, name))
    }

    /// [`Marks::want`] for a row standing for several agents: what any of
    /// those substituted.
    pub fn want_by<S: AsRef<str>>(&self, agents: &[S], name: &str) -> String {
        let spans = agents.iter().fold(vec![], |acc, agent| {
            merge_spans(&acc, Self::spans(&self.want, agent.as_ref(), name))
        });
        mark_spans(name, &spans)
    }

    /// `name` as a condition of the promise by which `agent` keeps
    /// `promise`: as the condition is written, with the values of the goal
    /// `promise` filled in, as resolution reached it with a sibling's value,
    /// or a behavior the condition's `{{?x}}` matched. `promise` is the
    /// behavior kept — for a parameterized promise listed on its own, the
    /// pattern as written.
    ///
    /// Another promise of the same agent with a condition of the same text
    /// adds nothing. Two promises of one agent that keep the same behavior
    /// and carry a condition of the same text are not told apart.
    pub fn condition(&self, agent: &str, promise: &str, name: &str) -> String {
        mark_spans(name, Self::spans3(&self.condition, agent, promise, name))
    }

    /// [`Marks::condition`] for a row standing for several agents.
    pub fn condition_by<S: AsRef<str>>(&self, agents: &[S], promise: &str, name: &str) -> String {
        let spans = agents.iter().fold(vec![], |acc, agent| {
            merge_spans(
                &acc,
                Self::spans3(&self.condition, agent.as_ref(), promise, name),
            )
        });
        mark_spans(name, &spans)
    }

    /// `kept` as the promise `agent` wrote as `written` names it: what that
    /// one promise substituted. For the promise as it stands in the
    /// contract, pattern or not, `written` and `kept` are the same text.
    /// Another promise of the same agent keeping the same behavior adds
    /// nothing: an outright `x is up` stays plain beside `{{who}} is up`.
    pub fn made(&self, agent: &str, written: &str, kept: &str) -> String {
        mark_spans(kept, Self::spans3(&self.made, agent, written, kept))
    }

    /// [`Marks::made`] for a row standing for several agents.
    pub fn made_by<S: AsRef<str>>(&self, agents: &[S], written: &str, kept: &str) -> String {
        let spans = agents.iter().fold(vec![], |acc, agent| {
            merge_spans(
                &acc,
                Self::spans3(&self.made, agent.as_ref(), written, kept),
            )
        });
        mark_spans(kept, &spans)
    }

    /// `name` as `agent` needs it: its want, or a condition on any of its
    /// promises. The plain name when the agent substituted nothing.
    ///
    /// By agent and name only, for a line or node that stands for both. A
    /// row for one want or one promise's condition reads [`Marks::want`] or
    /// [`Marks::condition`].
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
    /// parameterized promise as written. Every promise of the agent that
    /// keeps the name counts; [`Marks::made`] reads one of them.
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

    /// x keeps p1 on a condition written out, and p2 on an open one that
    /// matches the same behavior.
    const TWO_WAYS: &str = r#"
kind: Agent
name: x
provides:
- name: p1
  conditions: ["é1 is up"]
- name: p2
  conditions: ["{{?w}} is up"]
---
kind: Agent
name: "é1"
provides:
- name: "é1 is up"
"#;

    #[test]
    fn a_condition_written_out_is_not_marked_by_a_sibling_promise_that_matched_it() {
        let m = Marks::new(&tracker(TWO_WAYS));
        // p1 names it outright; only p2 found it through a variable.
        assert_eq!(m.condition("x", "p1", "\u{e9}1 is up"), "\u{e9}1 is up");
        assert_eq!(m.condition("x", "p2", "\u{e9}1 is up"), "<\u{e9}1> is up");
        assert_eq!(m.condition("x", "p2", "{{?w}} is up"), "{{?w}} is up");
        assert_eq!(
            m.condition_by(&["x", "\u{e9}1"], "p1", "\u{e9}1 is up"),
            "\u{e9}1 is up"
        );
        assert_eq!(
            m.condition_by(&["\u{e9}1", "x"], "p2", "\u{e9}1 is up"),
            "<\u{e9}1> is up"
        );
        // By agent and name alone the two cannot be told apart: the
        // Overview's one line from x for this behavior shows both.
        assert_eq!(m.need("x", "\u{e9}1 is up"), "<\u{e9}1> is up");
        // x wants nothing.
        assert_eq!(m.want("x", "\u{e9}1 is up"), "\u{e9}1 is up");
    }

    #[test]
    fn a_want_is_not_marked_by_a_condition_of_the_same_text() {
        let m = Marks::new(&tracker(
            r#"
kind: Agent
name: a1
wants:
- name: a2 is up
- name: "{{!self}} is fed"
provides:
- name: p
  conditions: ["{{?who}} is up"]
---
kind: Agent
name: a2
wants:
- name: "{{!self}} is up"
provides:
- name: a2 is up
"#,
        ));
        // a1 wrote the want out; its promise's `{{?who}}` found the same text.
        assert_eq!(m.want("a1", "a2 is up"), "a2 is up");
        assert_eq!(m.condition("a1", "p", "a2 is up"), "<a2> is up");
        assert_eq!(m.need("a1", "a2 is up"), "<a2> is up");
        assert_eq!(m.want("a2", "a2 is up"), "<a2> is up");
        assert_eq!(m.want("a1", "a1 is fed"), "<a1> is fed");
        // A row for both wanters shows what either wrote.
        assert_eq!(m.want_by(&["a1", "a2"], "a2 is up"), "<a2> is up");
        assert_eq!(m.want_by(&["a1"], "a2 is up"), "a2 is up");
        assert_eq!(m.want_by::<&str>(&[], "a2 is up"), "a2 is up");
    }

    #[test]
    fn a_promise_written_out_is_not_marked_by_a_pattern_keeping_the_same_behavior() {
        let m = Marks::new(&tracker(
            r#"
kind: Agent
name: s
provides:
- name: x is up
- name: "{{who}} is up"
  conditions: ["{{who}} is fed"]
---
kind: Agent
name: t
provides:
- name: "{{!self}} is up"
---
kind: Agent
name: w
wants:
- name: x is up
"#,
        ));
        assert_eq!(m.made("s", "x is up", "x is up"), "x is up");
        assert_eq!(m.made("s", "{{who}} is up", "x is up"), "<x> is up");
        assert_eq!(m.made("s", "{{who}} is up", "{{who}} is up"), "{{who}} is up");
        // The condition under the behavior the pattern keeps, and as written.
        assert_eq!(m.condition("s", "x is up", "x is fed"), "<x> is fed");
        assert_eq!(
            m.condition("s", "{{who}} is up", "{{who}} is fed"),
            "{{who}} is fed"
        );
        // A promise naming its own agent is marked from its declaration.
        assert_eq!(m.made("t", "t is up", "t is up"), "<t> is up");
        assert_eq!(m.made_by(&["s", "t"], "t is up", "t is up"), "<t> is up");
        assert_eq!(m.made_by(&["s", "t"], "x is up", "x is up"), "x is up");
        // By agent and name alone, s's two promises are one.
        assert_eq!(m.promise("s", "x is up"), "<x> is up");
    }

    #[test]
    fn the_finer_marks_cover_the_iam_chain() {
        let m = Marks::new(&tracker(IAM));
        assert_eq!(
            m.want("appname", WANT),
            "Read Secret Contents appsecret by <appname>"
        );
        assert_eq!(m.want("AWSSM", WANT), WANT);
        let pattern = "Read Secret Contents {{secret_name}} by {{application}}";
        assert_eq!(
            m.made("AWSSM", pattern, WANT),
            "Read Secret Contents <appsecret> by <appname>"
        );
        assert_eq!(m.made("AWSSM", pattern, pattern), pattern);
        assert_eq!(m.made("appname", ROLE, ROLE), "<appname> has IAM role approle");
        // AWSSM's conditions, under the behavior its promise keeps: with the
        // goal's values, with the sibling's too, and the behavior matched.
        assert_eq!(
            m.condition("AWSSM", WANT, OPEN_ROLE),
            "<appname> has IAM role {{?approle}}"
        );
        assert_eq!(
            m.condition(
                "AWSSM",
                WANT,
                "IAM access granted for reading Secret appsecret to {{approle}}"
            ),
            "IAM access granted for reading Secret <appsecret> to {{approle}}"
        );
        assert_eq!(
            m.condition("AWSSM", WANT, GRANT),
            "IAM access granted for reading Secret <appsecret> to <approle>"
        );
        assert_eq!(
            m.condition("AWSSM", WANT, ROLE),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(
            m.condition("infrateam", GRANT, APPROVAL),
            "Design approval access to Secret <appsecret> for <appname>"
        );
        // Under another promise the same text says nothing.
        assert_eq!(m.condition("AWSSM", GRANT, ROLE), ROLE);
        assert_eq!(m.condition("infrateam", WANT, APPROVAL), APPROVAL);
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
