use crate::components::behavior::Behavior;
use crate::components::pattern::{Bindings, Pattern};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct IntermediateAgent {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Behavior>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wants: Vec<Behavior>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub global_conditions: Vec<String>,
}

impl From<Agent> for IntermediateAgent {
    fn from(value: Agent) -> Self {
        let mut conditions = value.get_conditions().into_iter().collect::<Vec<String>>();
        conditions.sort();
        let mut global_conditions = vec![];
        for c in conditions {
            let mut inuse = true;
            for p in value.provides.iter() {
                if !p.get_conditions().contains(&c) {
                    inuse = false;
                    break;
                }
            }
            if inuse {
                global_conditions.push(c);
            }
        }
        let provides = value
            .provides
            .iter()
            .map(|p| {
                let conditions = p
                    .get_conditions()
                    .iter()
                    .filter(|c| !global_conditions.contains(c))
                    .cloned()
                    .collect();
                Behavior::new(p.get_name().clone()).with_conditions(conditions)
            })
            .collect::<Vec<Behavior>>();

        IntermediateAgent {
            name: value.name,
            comment: value.comment,
            provides: provides,
            wants: value.wants,
            global_conditions: global_conditions,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "IntermediateAgent")]
#[serde(into = "IntermediateAgent")]
pub struct Agent {
    name: String,
    #[serde(default)]
    comment: String,

    #[serde(default)]
    provides: Vec<Behavior>,

    #[serde(default)]
    wants: Vec<Behavior>,
}

impl TryFrom<IntermediateAgent> for Agent {
    type Error = String;

    fn try_from(value: IntermediateAgent) -> Result<Self, Self::Error> {
        let mut provides = value.provides.clone();
        for p in &mut provides {
            for c in &value.global_conditions {
                p.add_condition(c.clone());
            }
        }

        Ok(Agent {
            name: value.name,
            comment: value.comment,
            provides: provides,
            wants: value.wants,
        })
    }
}

impl Agent {
    pub fn new(name: String) -> Agent {
        Agent {
            name: name,
            comment: String::from(""),
            provides: vec![],
            wants: vec![],
        }
    }

    pub fn build(name: &str) -> Agent {
        Agent::new(String::from(name))
    }

    pub fn with_provides(mut self, provides: Vec<Behavior>) -> Agent {
        self.provides = provides;
        self
    }

    pub fn with_wants(mut self, wants: Vec<Behavior>) -> Agent {
        self.wants = wants;
        self
    }

    // Does not provide a global_conditions since that could be modified after the fact

    pub fn get_name(&self) -> &String {
        &self.name
    }

    /// What this agent promises, as declared.
    pub fn provides(&self) -> &[Behavior] {
        &self.provides
    }

    /// What this agent needs, as declared.
    pub fn wants(&self) -> &[Behavior] {
        &self.wants
    }

    pub fn is_wants_empty(&self) -> bool {
        self.wants.len() == 0
    }

    pub fn add_provide(&mut self, p: Behavior) {
        self.provides.push(p)
    }

    pub fn add_want(&mut self, w: Behavior) {
        self.wants.push(w)
    }

    pub fn has_behavior(&self, behavior_name: &String) -> bool {
        self.provides.iter().any(|x| x.has_behavior(behavior_name))
            || self.wants.iter().any(|x| x.get_name() == behavior_name)
    }

    pub fn get_conditions(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for p in &self.provides {
            for c in p.get_conditions() {
                ret.insert(c.clone());
            }
        }
        ret
    }

    pub fn get_wants(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for w in &self.wants {
            ret.insert(w.get_name().clone());
        }
        ret
    }

    /// Every promise this agent could keep for `goal`, with the bindings that
    /// goal implies for the promise's variables.
    ///
    /// Declaration order rather than a set: one agent may declare the same
    /// behavior twice, and two matches of one pattern carry different bindings.
    pub fn get_matching_provides(&self, goal: &str) -> Vec<(Behavior, Bindings)> {
        self.provides
            .iter()
            .filter_map(|b| b.match_goal(goal).map(|bindings| (b.clone(), bindings)))
            .collect()
    }

    /// The promises this agent describes with variables still in them.
    pub fn get_provide_patterns(&self) -> Vec<&Behavior> {
        self.provides
            .iter()
            .filter(|b| !b.get_name_pattern().is_ground())
            .collect()
    }

    pub fn get_all_provides(&self) -> HashSet<Behavior> {
        let mut ret = HashSet::new();
        for b in self.provides.iter() {
            ret.insert(b.clone());
        }
        ret
    }

    /// Every concrete behavior name this agent mentions.
    ///
    /// A parameterized name is left out: it is not a behavior, it stands for a
    /// family of them. Those come from [`Agent::get_behavior_patterns`].
    pub fn get_behaviors(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for p in &self.provides {
            if p.get_name_pattern().is_ground() {
                ret.insert(p.get_name().clone());
            }
            for c in p.get_condition_patterns() {
                if c.is_ground() {
                    ret.insert(c.source().clone());
                }
            }
        }
        for w in &self.wants {
            if w.get_name_pattern().is_ground() {
                ret.insert(w.get_name().clone());
            }
        }
        ret
    }

    /// The parameterized names this agent mentions, as written.
    pub fn get_behavior_patterns(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for p in &self.provides {
            if !p.get_name_pattern().is_ground() {
                ret.insert(p.get_name().clone());
            }
            for c in p.get_condition_patterns() {
                if !c.is_ground() {
                    ret.insert(c.source().clone());
                }
            }
        }
        ret
    }

    /// Could this agent take part in answering the concrete `behavior_name` —
    /// by promising it, needing it, or depending on it?
    ///
    /// A pattern that covers the name counts, which is what separates this from
    /// [`Agent::has_behavior`] and its literal reading.
    pub fn has_ground_behavior(&self, behavior_name: &str) -> bool {
        self.provides.iter().any(|p| {
            p.match_goal(behavior_name).is_some()
                || p.get_condition_patterns()
                    .iter()
                    .any(|c| c.match_ground(behavior_name).is_some())
        }) || self.wants.iter().any(|w| w.get_name() == behavior_name)
    }

    pub fn merge(&mut self, other: &Agent) {
        for p in &other.provides {
            if self.provides.contains(p) {
                continue;
            }
            self.provides.push(p.clone());
        }
        for w in &other.wants {
            if self.wants.contains(w) {
                continue;
            }
            self.wants.push(w.clone());
        }
    }

    /// Replace every condition the agent already meets itself with whatever
    /// meeting it depends on from outside.
    pub fn reduce(&mut self) {
        let originals = self.provides.clone();
        let mut reduced: Vec<Behavior> = originals
            .iter()
            .map(|b| reduce_one(b, &originals))
            .collect();
        reduced.sort();
        self.provides = reduced;
    }

    /// A copy of this agent under an instance name, with `bindings`
    /// substituted through everything it promises and needs.
    ///
    /// What the bindings do not cover stays parameterized, so a copy can fix
    /// some values and leave the rest to whoever wants the promise.
    pub fn make_instance(&self, instance_name: &str, bindings: &Bindings) -> Agent {
        Agent::new(instance_name.to_string())
            .with_provides(
                self.provides
                    .iter()
                    .map(|p| p.instantiate(bindings))
                    .collect(),
            )
            .with_wants(self.wants.iter().map(|w| w.instantiate(bindings)).collect())
    }
}

/// How many condition expansions one promise may go through before reduction
/// gives up and passes the rest through untouched. A parameterized condition
/// can name something longer than itself, so expansion is not guaranteed to
/// settle on its own.
const MAX_REDUCE_STEPS: usize = 1_000;

/// One promise with its internally-met conditions replaced by their own
/// external conditions, transitively.
fn reduce_one(behavior: &Behavior, provides: &[Behavior]) -> Behavior {
    // Conditions already dealt with. Seeding it with the promise's own name
    // drops a self-referential condition instead of looping on it.
    let mut seen: HashSet<String> = HashSet::from([behavior.get_name().clone()]);
    let mut queue: Vec<Pattern> = behavior.get_condition_patterns().to_vec();
    let mut external: BTreeSet<Pattern> = BTreeSet::new();
    let mut steps = 0;

    while let Some(condition) = queue.pop() {
        steps += 1;
        if steps > MAX_REDUCE_STEPS {
            external.insert(condition);
            external.extend(queue.drain(..));
            break;
        }
        if !seen.insert(condition.source().clone()) {
            continue;
        }
        match internally_met(&condition, provides) {
            Some(inner) => queue.extend(inner),
            None => {
                external.insert(condition);
            }
        }
    }

    behavior
        .clone()
        .with_condition_patterns(external.into_iter().collect())
}

/// If one of `provides` already covers `condition`, what that promise depends
/// on in turn, with the condition's own values substituted through.
///
/// Only ground conditions are expanded. A parameterized condition would have to
/// be matched against a parameterized provider — two open sides, the general
/// unification this design avoids — so it passes through as external.
fn internally_met(condition: &Pattern, provides: &[Behavior]) -> Option<Vec<Pattern>> {
    if !condition.is_ground() {
        return None;
    }
    // Where several promises could cover it, the first declared wins.
    for candidate in provides {
        if let Some(bindings) = candidate.match_goal(condition.source()) {
            return Some(
                candidate
                    .get_condition_patterns()
                    .iter()
                    .map(|c| c.substitute(&bindings))
                    .collect(),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::{self};

    #[test]
    fn simple() {
        let mut a = Agent::new(String::from("foo"));
        assert_eq!(a.name, "foo");
        assert_eq!(a.comment, "");
        assert_eq!(a.get_name(), "foo");
        assert!(a.is_wants_empty());

        a.add_want(Behavior::new(String::from("w1")));
        assert_eq!(a.wants, vec!(Behavior::new(String::from("w1"))));
        assert_eq!(a.get_wants(), HashSet::from([String::from("w1")]));
        assert!(!a.is_wants_empty());

        assert_eq!(a.provides, vec!());
        a.add_provide(Behavior::new(String::from("p1")));
        a.add_provide(Behavior::new_with_conditions(
            String::from("p2"),
            vec![String::from("c1"), String::from("c2")],
        ));
        assert!(a.has_behavior(&String::from("p1")));
        assert!(a.has_behavior(&String::from("p2")));
        assert!(a.has_behavior(&String::from("c1")));
        assert!(a.has_behavior(&String::from("c2")));
        assert!(!a.has_behavior(&String::from("c3")));
        assert!(a.has_behavior(&String::from("w1")));
        assert!(!a.has_behavior(&String::from("w2")));
        assert_eq!(
            a.provides,
            vec!(
                Behavior::new(String::from("p1")),
                Behavior::new_with_conditions(
                    String::from("p2"),
                    vec!(String::from("c1"), String::from("c2")),
                ),
            ),
        );
    }

    #[test]
    fn simple_from_yaml() {
        let a: Agent = serde_yaml::from_str(
            "name: foo
comment: this is a comment
provides:
  - name: p2
    conditions:
      - c2
      - c1
  - name: p1
wants:
  - name: w2
  - name: w1
",
        )
        .expect("Unable to parse");
        assert_eq!(a.name, "foo");
        assert_eq!(a.comment, "this is a comment");
        assert_eq!(
            a.provides,
            vec!(
                Behavior::new_with_conditions(
                    String::from("p2"),
                    vec!(String::from("c2"), String::from("c1"))
                ),
                Behavior::new(String::from("p1")),
            )
        );
        assert_eq!(
            a.wants,
            vec!(
                Behavior::new(String::from("w2")),
                Behavior::new(String::from("w1")),
            )
        );
    }

    #[test]
    fn from_yaml_with_global_conditions() {
        let a: Agent = serde_yaml::from_str(
            "name: foo
comment: this is a comment
provides:
  - name: p1
  - name: p2
    conditions:
      - c2
      - c1
wants:
  - name: w1
  - name: w2
globalConditions:
  - gc1
  - gc2
",
        )
        .expect("Unable to parse");
        assert_eq!(a.name, "foo");
        assert_eq!(a.comment, "this is a comment");
        assert_eq!(
            a.provides,
            vec!(
                Behavior::new_with_conditions(
                    String::from("p1"),
                    vec!(String::from("gc1"), String::from("gc2"))
                ),
                Behavior::new_with_conditions(
                    String::from("p2"),
                    vec!(
                        String::from("c2"),
                        String::from("c1"),
                        String::from("gc1"),
                        String::from("gc2")
                    )
                ),
            ),
        );
    }

    #[test]
    fn to_yaml_simple() {
        let a = Agent::new(String::from("foo"))
            .with_provides(vec![
                Behavior::new(String::from("p1"))
                    .with_conditions(vec![String::from("p1c1"), String::from("p1c2")]),
                Behavior::new(String::from("p2")),
            ])
            .with_wants(vec![
                Behavior::new(String::from("w1")),
                Behavior::new(String::from("w2")),
            ]);
        let s = serde_yaml::to_string(&a).expect("Unable to serialize");
        let expected = "name: foo\nprovides:\n- name: p1\n  conditions:\n  - p1c1\n  - p1c2\n- name: p2\nwants:\n- name: w1\n- name: w2\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn to_yaml_with_global_conditions() {
        let a = Agent {
            name: String::from("foo"),
            comment: String::from(""),
            provides: vec![
                Behavior::new(String::from("p1")).with_conditions(vec![String::from("gc1")]),
                Behavior::new(String::from("p2")).with_conditions(vec![
                    String::from("c1"),
                    String::from("c2"),
                    String::from("gc1"),
                ]),
            ],
            wants: vec![
                Behavior::new(String::from("w1")),
                Behavior::new(String::from("w2")),
            ],
        };
        let s = serde_yaml::to_string(&a).expect("Unable to serialize");
        let expected = "name: foo\nprovides:\n- name: p1\n- name: p2\n  conditions:\n  - c1\n  - c2\nwants:\n- name: w1\n- name: w2\nglobalConditions:\n- gc1\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn get_conditions() {
        let a: Agent = serde_yaml::from_str(
            "name: foo
provides:
  - name: b3
    conditions:
      - c3
  - name: b1
    conditions:
      - c2
      - c1
  - name: b2
    conditions:
      - c4
  - name: b2
    conditions:
      - c2
",
        )
        .expect("Test parse failure");
        let expected: HashSet<String> = HashSet::from(["c1", "c2", "c3", "c4"])
            .iter()
            .map(|x| x.to_string())
            .collect();
        assert_eq!(a.get_conditions(), expected);
    }

    #[test]
    fn get_behaviors() {
        let a: Agent = serde_yaml::from_str(
            "name: foo
provides:
  - name: b3
    conditions:
      - c3
  - name: b1
    conditions:
      - c2
      - c1
  - name: b2
    conditions:
      - c4
  - name: b2
    conditions:
      - c2
wants:
  - name: w1
  - name: w1
  - name: w2
",
        )
        .expect("Test parse failure");
        let expected: HashSet<String> =
            HashSet::from(["b1", "b2", "b3", "c1", "c2", "c3", "c4", "w1", "w2"])
                .iter()
                .map(|x| x.to_string())
                .collect();
        assert_eq!(a.get_behaviors(), expected);
    }

    #[test]
    fn test_get_all_provides() {
        let a = Agent::build("foo").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("c1")]),
            Behavior::build("b2").with_conditions(vec![String::from("c2")]),
        ]);
        assert_eq!(
            a.get_all_provides(),
            HashSet::from([
                Behavior::build("b1").with_conditions(vec![String::from("c1")]),
                Behavior::build("b2").with_conditions(vec![String::from("c2")]),
            ])
        );
    }

    #[test]
    fn test_merge() {
        let mut a = Agent::new(String::from("foo")).with_provides(vec![
            Behavior::new(String::from("b1")),
            Behavior::new(String::from("b2")),
        ]);
        a.merge(&Agent::new(String::from("bar")).with_provides(vec![
            Behavior::new(String::from("b2")),
            Behavior::new(String::from("b3")),
        ]));
        assert_eq!(
            a.provides,
            vec!(
                Behavior::new(String::from("b1")),
                Behavior::new(String::from("b2")),
                Behavior::new(String::from("b3")),
            )
        );
    }

    #[test]
    fn test_reduce() {
        let mut a: Agent = serde_yaml::from_str(
            "name: foo
provides:
  - name: b1
    conditions:
      - b2
  - name: b2
    conditions:
      - b3
",
        )
        .unwrap();
        a.reduce();
        assert_eq!(
            a.provides,
            vec!(
                Behavior::build("b1").with_conditions(vec!(String::from("b3"))),
                Behavior::build("b2").with_conditions(vec!(String::from("b3"))),
            )
        );

        let mut a: Agent = serde_yaml::from_str(
            "name: foo
provides:
  - name: b1
    conditions:
      - b2
  - name: b2
    conditions:
      - b3
  - name: b2
    conditions:
      - b4
",
        )
        .unwrap();
        a.reduce();
        assert_eq!(
            a.provides,
            vec!(
                Behavior::build("b1").with_conditions(vec!(String::from("b3"))),
                Behavior::build("b2").with_conditions(vec!(String::from("b3"))),
                Behavior::build("b2").with_conditions(vec!(String::from("b4"))),
            )
        );
    }

    #[test]
    fn test_make_instance() {
        let a = Agent::new(String::from("a1"))
            .with_provides(vec![
                Behavior::new(String::from("p1"))
                    .with_conditions(vec![String::from("p1c1"), String::from("p1c2")]),
                Behavior::new(String::from("p2")),
            ])
            .with_wants(vec![
                Behavior::new(String::from("w1")),
                Behavior::new(String::from("w2")),
            ]);
        let result = a.make_instance("i1", &Bindings::new());
        assert_eq!(
            result,
            Agent::new(String::from("i1"))
                .with_provides(vec![
                    Behavior::new(String::from("p1"))
                        .with_conditions(vec![String::from("p1c1"), String::from("p1c2")]),
                    Behavior::new(String::from("p2")),
                ])
                .with_wants(vec![
                    Behavior::new(String::from("w1")),
                    Behavior::new(String::from("w2")),
                ])
        );
    }

    #[test]
    fn test_make_instance_global_conditions() {
        let ia = IntermediateAgent {
            name: String::from("a1"),
            comment: String::from(""),
            provides: vec![Behavior::new(String::from("p1"))
                .with_conditions(vec![String::from("p1c1"), String::from("p1c2")])],
            wants: vec![Behavior::new(String::from("w1"))],
            global_conditions: vec![String::from("gc1"), String::from("gc2")],
        };
        let a = Agent::try_from(ia).unwrap();
        assert_eq!(
            a,
            Agent::new(String::from("a1"))
                .with_provides(vec![Behavior::new(String::from("p1")).with_conditions(
                    vec![
                        String::from("p1c1"),
                        String::from("p1c2"),
                        String::from("gc1"),
                        String::from("gc2"),
                    ]
                )])
                .with_wants(vec![Behavior::new(String::from("w1"))])
        );
    }

    #[test]
    fn test_reduce_leaves_parameterized_conditions() {
        let mut a: Agent = serde_yaml::from_str(
            "name: sa
provides:
  - name: outer
    conditions:
      - inner/{{v}}
  - name: inner/{{v}}
",
        )
        .unwrap();
        a.reduce();
        // `inner/{{v}}` is not ground, so it survives even though the agent
        // looks like it provides it: expanding would need matching an open
        // pattern against an open pattern.
        assert_eq!(
            a.provides,
            vec![
                Behavior::build("inner/{{v}}"),
                Behavior::build("outer").with_conditions(vec![String::from("inner/{{v}}")]),
            ]
        );
    }

    #[test]
    fn test_reduce_expands_a_ground_condition_through_a_pattern() {
        let mut a: Agent = serde_yaml::from_str(
            "name: sa
provides:
  - name: outer
    conditions:
      - inner/p1
  - name: inner/{{v}}
    conditions:
      - external/{{v}}
",
        )
        .unwrap();
        a.reduce();
        // The ground condition binds v = p1, and what `inner` needs in turn
        // comes out carrying that value.
        assert_eq!(
            a.provides,
            vec![
                Behavior::build("inner/{{v}}")
                    .with_conditions(vec![String::from("external/{{v}}")]),
                Behavior::build("outer").with_conditions(vec![String::from("external/p1")]),
            ]
        );
    }

    #[test]
    fn test_reduce_drops_a_self_referential_condition() {
        let mut a: Agent = serde_yaml::from_str(
            "name: sa
provides:
  - name: b1
    conditions:
      - b1
",
        )
        .unwrap();
        a.reduce();
        assert_eq!(a.provides, vec![Behavior::build("b1")]);
    }
}
