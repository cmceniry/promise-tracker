
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod components;

use components::SuperAgent;
use components::Agent;
use std::collections::HashMap;

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

  pub fn get_working_behaviors(&self) -> Vec<String> {
    let mut ret = vec!();
    for (_, a) in &self.working_agents {
      for c in a.get_conditions() {
        c.get
      }
    }
    ret.sort();
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
    t.add_agent(Agent::new(String::from("abcd")));
    t.add_agent(Agent::new(String::from("ijkl")));
    let mut a = Agent::new(String::from("efgh"));
    a.add_provide(Behavior::new_with_conditions(String::from("b1"), vec!(String::from("c1"))));
    // a.add_provide(Behavior::new_with_conditions(name: String::from("b1"), conditions: vec!(String::from("c1"))));
    // a.add_provide(Behavior::new_with_conditions(name: String::from("b2"), conditions: vec!(String::from("c2"))));
    t.add_agent(a);

    assert_eq!(t.get_agent_names(), vec!("abcd", "efgh", "ijkl"));
    assert_eq!(t.get_working_agent_names(), vec!("abcd", "efgh", "ijkl"));
    assert_eq!(t.get_working_behaviors(), vec!["b1", "b2"]);
  }
}
