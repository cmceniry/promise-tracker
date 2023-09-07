
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

impl Tracker {
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
    assert!(true);
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
}
