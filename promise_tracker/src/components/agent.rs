use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::{collections::HashSet, hash::Hash};
use crate::components::behavior::Behavior;

#[derive(Debug,PartialEq,Eq,Clone)]
#[derive(Deserialize,Serialize)]
#[derive(JsonSchema)]
pub struct Agent {
  name: String,

  #[serde(default)]
  provides: Vec<Behavior>,

  #[serde(default)]
  wants: Vec<Behavior>,
}

impl Agent {
  pub fn new(name: String) -> Agent {
    Agent{
      name: name,
      provides: vec!(),
      wants: vec!(),
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
    self.provides.iter().any(|x| x.has_behavior(behavior_name)) ||
    self.wants.iter().any(|x| x.get_name() == behavior_name)
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

}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_yaml::{self};

  #[test]
  fn simple() {
    let mut a = Agent::new(String::from("foo"));
    assert_eq!(a.name, "foo");
    assert_eq!(a.get_name(), "foo");
    assert!(a.is_wants_empty());

    a.add_want(Behavior::new(String::from("w1")));
    assert_eq!(a.wants, vec!(Behavior::new(String::from("w1"))));
    assert_eq!(a.get_wants(), HashSet::from([String::from("w1")]));
    assert!(!a.is_wants_empty());

    assert_eq!(a.provides, vec!());
    a.add_provide(Behavior::new(String::from("p1")));
    a.add_provide(Behavior::new_with_conditions(String::from("p2"), vec!(String::from("c1"), String::from("c2"))));
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
    let a:Agent = serde_yaml::from_str("name: foo
provides:
  - name: p2
    conditions:
      - c2
      - c1
  - name: p1
wants:
  - name: w2
  - name: w1
").expect("Unable to parse");
    assert_eq!(a.name, "foo");
    assert_eq!(a.provides, vec!(
      Behavior::new_with_conditions(String::from("p2"), vec!(String::from("c2"), String::from("c1"))),
      Behavior::new(String::from("p1")),
    ));
    assert_eq!(a.wants, vec!(
      Behavior::new(String::from("w2")),
      Behavior::new(String::from("w1")),
    ));
  }

  #[test]
  fn get_conditions() {
    let a:Agent = serde_yaml::from_str("name: foo
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
").expect("Test parse failure");
    let expected: HashSet<String> = HashSet::from(["c1", "c2", "c3", "c4"])
      .iter()
      .map(|x| x.to_string())
      .collect();
    assert_eq!(a.get_conditions(), expected);
  }

  #[test]
  fn get_behaviors() {
    let a:Agent = serde_yaml::from_str("name: foo
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
").expect("Test parse failure");
    let expected: HashSet<String> = HashSet::from(["b1", "b2", "b3", "c1", "c2", "c3", "c4", "w1", "w2"])
      .iter()
      .map(|x| x.to_string())
      .collect();
    assert_eq!(a.get_behaviors(), expected);
  }

}
