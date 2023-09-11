
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod components;

use components::SuperAgent;
use components::Agent;
use std::collections::HashMap;
use std::collections::HashSet;

pub struct Tracker {
  available_agents: Vec<Agent>,
  available_superagents: Vec<SuperAgent>,
  working_agents: HashMap<String, Vec<Agent>>,
}

// Need:
// - TODO - schema validation  - ContractCarder
// - TODO - tracker.resolve(behavior_name, root_component)
// - TODO ptdiagram?

impl Tracker {
  pub fn new() -> Tracker {
    Tracker{
      available_agents: vec!(),
      available_superagents: vec!(),
      working_agents: HashMap::new(),
    }
  }

  pub fn is_empty(&self) -> bool {
    self.working_agents.len() == 0
  }

  pub fn add_agent(&mut self, a: Agent) {
    for existing in &self.available_agents {
      if existing == &a {
        return;
      }
    }
    let _ = &self.available_agents.push(a.clone());
    self.add_working_agent(a);
  }

  pub fn get_agent_names(&self) -> Vec<&String> {
    let mut ret = vec!();
    for a in &self.available_agents {
      ret.push(a.get_name());
    }
    ret.sort();
    ret
  }

  pub fn has_agent(&self, agent_name: String) -> bool {
    self.working_agents.contains_key(&agent_name)
  }

  pub fn has_behavior(&self, behavior_name: String) -> bool {
    self.working_agents.iter().any(|(_, variants)| {
      variants.iter().any(|a| a.has_behavior(&behavior_name))
    })
  }

  pub fn is_agent_wants_empty(&self, agent_name: String) -> bool {
    let Some(&ref varients) = self.working_agents.get(&agent_name) else { todo!() };
    varients.iter().all(|a| a.is_wants_empty())
  }

  pub fn add_working_agent(&mut self, a: Agent) {
    let wa_list = self.working_agents.entry(a.get_name().clone()).or_insert(vec!());
    wa_list.push(a);
  }

  pub fn get_working_agent_names(&self) -> Vec<&String> {
    let mut ret = vec!();
    for (n, _) in &self.working_agents {
      ret.push(n);
    }
    ret.sort();
    ret
  }

  pub fn get_agent_wants(&self, agent_name: String) -> HashSet<String> {
    let mut ret = HashSet::new();
    let Some(&ref variants) = self.working_agents.get(&agent_name) else { todo!() };
    for varient in variants {
      ret.extend(varient.get_wants());
    }
    ret
  }

  pub fn get_working_behaviors(&self) -> HashSet<String> {
    let mut ret = HashSet::new();
    for (_, variants) in &self.working_agents {
      for variant_agent in variants {
        ret.extend(variant_agent.get_behaviors());
      }
    }
    ret
  }

}

#[cfg(test)]
mod tests {
  use super::*;
  use components::Behavior;

  #[test]
  fn simple_adds() {
    let mut t = Tracker{
      available_agents: vec!(),
      available_superagents: vec!(),
      working_agents: HashMap::new(),
    };
    let mut a = Agent::new(String::from("abcd"));
    a.add_provide(Behavior::new_with_conditions(String::from("ba"), vec!()));
    t.add_agent(a);
    t.add_agent(Agent::new(String::from("ijkl")));
    let mut b = Agent::new(String::from("efgh"));
    b.add_provide(Behavior::new_with_conditions(String::from("b1"), vec!(String::from("c1"))));
    b.add_provide(Behavior::new_with_conditions(String::from("b2"), vec!(String::from("c2"))));
    t.add_agent(b);

    assert_eq!(t.get_agent_names(), vec!("abcd", "efgh", "ijkl"));
    assert_eq!(t.get_working_agent_names(), vec!("abcd", "efgh", "ijkl"));
    let expected_behaviors: HashSet<String> = HashSet::from(["b1", "b2", "ba", "c1", "c2"])
      .iter()
      .map(|x| x.to_string())
      .collect();
    assert_eq!(t.get_working_behaviors(), expected_behaviors);
  }

  #[test]
  fn agent_bools() {
    let mut t = Tracker::new();

    assert!(t.is_empty());

    t.add_agent(Agent::new(String::from("abcd")));
    assert!(t.has_agent(String::from("abcd")));
    assert!(!t.has_agent(String::from("efgh")));
    assert!(t.is_agent_wants_empty(String::from("abcd")));

    t.add_agent(Agent::new(String::from("efgh")));
    assert!(t.has_agent(String::from("efgh")));
    let mut efgh = Agent::new(String::from("efgh"));
    efgh.add_want(Behavior::new(String::from("efgh_want1")));
    t.add_agent(efgh);
    assert!(!t.is_agent_wants_empty(String::from("efgh")));

    assert!(t.has_behavior(String::from("efgh_want1")));
    assert!(!t.has_behavior(String::from("missing_want")));
  }

  #[test]
  fn nested_gets() {
    let mut t = Tracker::new();

    let mut abcd = Agent::new(String::from("abcd"));
    abcd.add_want(Behavior::new(String::from("abcd_w1")));
    t.add_agent(abcd);
    let mut abcd = Agent::new(String::from("abcd"));
    abcd.add_want(Behavior::new(String::from("abcd_w2")));
    t.add_agent(abcd);

    let mut efgh = Agent::new(String::from("efgh"));
    efgh.add_want(Behavior::new(String::from("efgh_w3")));
    t.add_agent(efgh);

    assert_eq!(t.get_agent_wants(String::from("abcd")), HashSet::from([
      String::from("abcd_w1"),
      String::from("abcd_w2"),
    ]));
    assert_eq!(t.get_agent_wants(String::from("efgh")), HashSet::from([
      String::from("efgh_w3"),
    ]));
  }

}
