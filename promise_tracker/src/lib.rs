
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod components;

use components::SuperAgent;
use components::Agent;
use std::collections::HashMap;
use std::collections::HashSet;

pub mod resolve;
use resolve::Resolution;
use resolve::Offer;

#[derive(Debug)]
pub struct Tracker {
  available_agents: Vec<Agent>,
  available_superagents: Vec<SuperAgent>,
  working_agents: HashMap<String, Vec<Agent>>,
}

// Need:
// - TODO - schema validation  - ContractCarder
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

  // As a rule of thumb:
  // - satisfied conditions will result in an Offer
  // - unsatisfied conditions will result in an Resolution
  pub fn resolve(&self, behavior_name: &str) -> Resolution {
    let mut r = Resolution::new(behavior_name);
    for (agent_name, variants) in &self.working_agents {
      for variant_agent in variants {
        if let Some(behaviors) = variant_agent.get_provides(behavior_name) {
          for b in behaviors {
            // if unconditional, add this as a satisfied Offer
            if b.is_unconditional() {
              r = r.add_satisfying_offer(Offer::new(agent_name));
              continue;
            }
            // resolve conditions
            let resolved_conditions = b.get_conditions().iter()
              .map(|c| self.resolve(c))
              .collect::<Vec<Resolution>>();
            // if all conditions are satisfied, add this as a satisfied Offer
            if resolved_conditions.iter().all(|x| x.is_satisfied()) {
              r = r.add_satisfying_offer(Offer::new_conditional(agent_name, resolved_conditions));
            // otherwise, add this as an unsatisfied Offer
            } else {
              r = r.add_unsatisfying_offer(Offer::new_conditional(agent_name, resolved_conditions));
            }
          }
        }
      }
    }
    r
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

  #[test]
  fn test_simple_resolve() {
    let mut t = Tracker::new();
    t.add_agent(Agent::build("a1")
      .with_provides(vec!(
        Behavior::build("b1"),
      ))
    );
    assert_eq!(t.resolve("b1"), Resolution::new("b1")
      .add_satisfying_offer(Offer::new("a1")));

    t.add_agent(Agent::build("a2")
      .with_provides(vec!(
        Behavior::build("b1")
          .with_conditions(vec!(String::from("b2"))),
      ))
    );
    assert_eq!(t.resolve("b1"), Resolution::new("b1")
      .add_satisfying_offer(Offer::new("a1"))
      .add_unsatisfying_offer(Offer::new_conditional("a2", vec!(Resolution::new("b2"))))
    );

    t.add_agent(Agent::build("a3")
      .with_provides(vec!(
        Behavior::build("b2")
      ))
    );
    assert_eq!(t.resolve("b1"), Resolution::new("b1")
      .add_satisfying_offer(Offer::new("a1"))
      .add_satisfying_offer(Offer::new_conditional("a2", vec!(Resolution::new("b2")
        .add_satisfying_offer(Offer::new("a3"))
      )))
    );
  }

  #[test]
  fn test_resolve_multiple_satisfying() {
    let mut t = Tracker::new();
    t.add_agent(Agent::build("a1")
      .with_provides(vec!(
        Behavior::build("b1"),
      ))
    );
    t.add_agent(Agent::build("a2")
      .with_provides(vec!(
        Behavior::build("b1"),
      ))
    );
    t.add_agent(Agent::build("a3")
      .with_provides(vec!(
        Behavior::build("b1"),
      ))
    );
    let resolve_b1 = Resolution::new("b1")
      .add_satisfying_offer(Offer::new("a1"))
      .add_satisfying_offer(Offer::new("a2"))
      .add_satisfying_offer(Offer::new("a3"));
    assert_eq!(t.resolve("b1"), resolve_b1);
  }

  #[test]
  fn test_resolve_unsatisfied() {
    let mut t = Tracker::new();
    t.add_agent(Agent::build("a1")
      .with_provides(vec!(
        Behavior::build("b1")
          .with_conditions(vec!(String::from("b2a"), String::from("b2b"))),
      ))
    );
    t.add_agent(Agent::build("a2")
      .with_provides(vec!(
        Behavior::build("b2a"),
      ))
    );

    assert_eq!(t.resolve("b1"), Resolution::new("b1")
      .add_unsatisfying_offer(
        Offer::new_conditional("a1", vec!(
          Resolution::new("b2a")
            .add_satisfying_offer(Offer::new("a2"))
          ,
          Resolution::new("b2b")
          ,
      )))
    )
  }

  #[test]
  fn test_resolve_deep() {
    let mut t = Tracker::new();
    t.add_agent(Agent::build("a1")
      .with_provides(vec!(
        Behavior::build("b1")
          .with_conditions(vec!(String::from("b2"))),
      ))
    );
    t.add_agent(Agent::build("a2")
      .with_provides(vec!(
        Behavior::build("b2")
          .with_conditions(vec!(String::from("b3"))),
      ))
    );
    t.add_agent(Agent::build("a3")
      .with_provides(vec!(
        Behavior::build("b3")
          .with_conditions(vec!(String::from("b4"))),
      ))
    );
    t.add_agent(Agent::build("a4")
      .with_provides(vec!(
        Behavior::build("b4"),
      ))
    );
    let satisfied_part = Resolution::new("b1")
      .add_satisfying_offer(
        Offer::new_conditional("a1", vec!(
          Resolution::new("b2")
            .add_satisfying_offer(
              Offer::new_conditional("a2", vec!(
                Resolution::new("b3")
                  .add_satisfying_offer(
                    Offer::new_conditional("a3", vec!(
                      Resolution::new("b4")
                        .add_satisfying_offer(Offer::new("a4"))
                    ))
                  )
              ))
            )
        ))
      );
    assert_eq!(t.resolve("b1"), satisfied_part);
    t.add_agent(Agent::build("a0")
      .with_provides(vec!(
        Behavior::build("b0")
          .with_conditions(vec!(
            String::from("b1"),
            String::from("b1b"),
          )),
      ))
    );
    t.add_agent(Agent::build("a1b")
      .with_provides(vec!(
        Behavior::build("b1b")
          .with_conditions(vec!(String::from("b2b"))),
      ))
    );
    assert_eq!(t.resolve("b0"), Resolution::new("b0")
      .add_unsatisfying_offer(Offer::new_conditional("a0", vec!(
        satisfied_part,
        Resolution::new("b1b")
          .add_unsatisfying_offer(
            Offer::new_conditional("a1b", vec!(
              Resolution::new("b2b")
            ))
          )
      )))
    );
  }

}
