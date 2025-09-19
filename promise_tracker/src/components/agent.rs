use crate::components::behavior::Behavior;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
        let provides = value.provides.iter().map(|p| {
            let conditions = p.get_conditions().iter().filter(|c| {
                !global_conditions.contains(c)
            }).cloned().collect();
            Behavior::new(p.get_name().clone()).with_conditions(conditions)
        }).collect::<Vec<Behavior>>();

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

    pub fn get_provides(&self, behavior_name: &str) -> Option<HashSet<Behavior>> {
        let mut ret = HashSet::new();
        for b in self.provides.iter() {
            if b.get_name() == behavior_name {
                ret.insert(b.clone());
            }
        }
        if ret.len() > 0 {
            Some(ret)
        } else {
            None
        }
    }

    pub fn get_all_provides(&self) -> HashSet<Behavior> {
        let mut ret = HashSet::new();
        for b in self.provides.iter() {
            ret.insert(b.clone());
        }
        ret
    }

    pub fn get_behaviors(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for p in &self.provides {
            ret.insert(p.get_name().clone());
            for c in p.get_conditions() {
                ret.insert(c.clone());
            }
        }
        for w in &self.wants {
            ret.insert(w.get_name().clone());
        }
        ret
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

    // for each condition that is internally provided, replace it with the conditions required to internally provide it
    pub fn reduce(&mut self) {
        let internal_provides: HashSet<String> =
            self.provides.iter().map(|p| p.get_name().clone()).collect();
        let mut todo_provides = self.provides.clone();
        let mut reduced_provides = vec![];
        while todo_provides.len() > 0 {
            let p = todo_provides.remove(0);
            if p.is_unconditional() || p.has_none_of_these_conditions(&internal_provides) {
                reduced_provides.push(p);
                continue;
            }
            // Otherwise, one or more conditions need to be expanded
            // If a condition is provided internally by multiple options, technically should expand each of them.
            // Right now, just expand the first one.
            let mut new_conditions: HashSet<String> = HashSet::new();
            for c in p.get_conditions() {
                // If this condition is not provided internally, just pass it through as is
                if !internal_provides.contains(&c) {
                    new_conditions.insert(c.clone());
                    continue;
                }
                for ro in self.provides.iter().filter(|x| x.get_name() == &c).take(1) {
                    new_conditions.extend(ro.get_conditions());
                }
            }
            todo_provides.push(Behavior::new_with_conditions(
                p.get_name().clone(),
                new_conditions.iter().map(|x| x.clone()).collect(),
            ));
        }
        reduced_provides.sort();
        self.provides = reduced_provides;
    }

    pub fn make_instance(
        &self,
        instance_name: &String,
        provides_tags: &String,
        conditions_tags: &String,
    ) -> Agent {
        Agent::new(instance_name.clone())
            .with_provides(
                self.provides
                    .iter()
                    .map(|p| p.make_instance(provides_tags, conditions_tags))
                    .collect(),
            )
            .with_wants(self.wants.clone())
    }
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
                    vec!(String::from("c2"), String::from("c1"), String::from("gc1"), String::from("gc2"))
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
                Behavior::new(
                    String::from("p1"),
                ).with_conditions(vec![String::from("gc1")]),
                Behavior::new(
                    String::from("p2"),
                ).with_conditions(vec![String::from("c1"), String::from("c2"), String::from("gc1")]),
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
        let result = a.make_instance(
            &String::from("i1"),
            &String::from("i1p"),
            &String::from("i1c"),
        );
        assert_eq!(
            result,
            Agent::new(String::from("i1"))
                .with_provides(vec![
                    Behavior::new(String::from("p1 | i1p")).with_conditions(vec![
                        String::from("p1c1 | i1c"),
                        String::from("p1c2 | i1c")
                    ]),
                    Behavior::new(String::from("p2 | i1p")),
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
            Agent::new(String::from("a1")).with_provides(vec![Behavior::new(String::from("p1"))
                .with_conditions(vec![
                    String::from("p1c1"),
                    String::from("p1c2"),
                    String::from("gc1"),
                    String::from("gc2"),
                ])])
                .with_wants(vec![Behavior::new(String::from("w1"))])
        );
    }
}
