pub mod components;

use components::Agent;
use components::Item;
use components::SuperAgent;
use std::collections::HashMap;
use std::collections::HashSet;

pub mod resolve;
use resolve::Offer;
use resolve::Resolution;

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
        Tracker {
            available_agents: vec![],
            available_superagents: vec![],
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
        self.rebuild();
    }

    pub fn add_superagent(&mut self, sa: SuperAgent) {
        for existing in &self.available_superagents {
            if existing == &sa {
                return;
            }
        }
        let _ = &self.available_superagents.push(sa.clone());
        self.rebuild();
    }

    pub fn add_item(&mut self, i: Item) {
        match i {
            Item::Agent(a) => self.add_agent(a),
            Item::SuperAgent(sa) => self.add_superagent(sa),
        }
    }

    pub fn rebuild(&mut self) {
        let mut new_working_agents: HashMap<String, Vec<Agent>> = HashMap::new();
        let mut all_contained_agent_names = HashSet::new();
        for sa in &self.available_superagents {
            let contained_agents_names = sa.get_agent_names();
            for contained_agent_name in contained_agents_names.iter() {
                all_contained_agent_names.insert(contained_agent_name.clone());
            }
            // build out a stub agent that is a combination of all of the contained agents
            let mut stub_agent = Agent::new(sa.get_name().clone());
            self.available_agents
                .iter()
                .filter(|a| contained_agents_names.contains(a.get_name()))
                .for_each(|a| {
                    stub_agent.merge(a);
                });
            // reduce its behaviors to those that are not internally handled
            stub_agent.reduce();

            // if there are instances of this sa, use those; otherwise use itself
            let instances = sa.get_instances();
            if instances.len() == 0 {
                let e = new_working_agents
                    .entry(stub_agent.get_name().clone())
                    .or_insert(vec![stub_agent.clone()]);
                e[0].merge(&stub_agent);
                continue;
            }
            for i in instances.iter() {
                let mut instance_agent = stub_agent.make_instance(
                    i.get_name(),
                    i.get_provides_tags(),
                    i.get_conditions_tags(),
                );
                for p in i.get_provides().iter() {
                    instance_agent.add_provide(p.clone());
                }
                for w in i.get_wants().iter() {
                    instance_agent.add_want(w.clone());
                }
                let e = new_working_agents
                    .entry(instance_agent.get_name().clone())
                    .or_insert(vec![instance_agent.clone()]);
                e[0].merge(&instance_agent);
            }
        }
        for a in &self.available_agents {
            if all_contained_agent_names.contains(a.get_name()) {
                continue;
            }
            let e = new_working_agents
                .entry(a.get_name().clone())
                .or_insert(vec![a.clone()]);
            e[0].merge(&a);
        }
        self.working_agents = new_working_agents;
    }

    pub fn get_agent_names(&self) -> Vec<&String> {
        let mut ret = vec![];
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
        self.working_agents
            .iter()
            .any(|(_, variants)| variants.iter().any(|a| a.has_behavior(&behavior_name)))
    }

    pub fn is_agent_wants_empty(&self, agent_name: String) -> bool {
        let Some(&ref varients) = self.working_agents.get(&agent_name) else {
            todo!()
        };
        varients.iter().all(|a| a.is_wants_empty())
    }

    pub fn get_working_agent_names(&self) -> Vec<&String> {
        let mut ret = vec![];
        for (n, _) in &self.working_agents {
            ret.push(n);
        }
        ret.sort();
        ret
    }

    pub fn get_agent_wants(&self, agent_name: String) -> HashSet<String> {
        let mut ret = HashSet::new();
        let Some(&ref variants) = self.working_agents.get(&agent_name) else {
            todo!()
        };
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
        let mut agent_names: Vec<String> = vec![];
        for (a, _) in &self.working_agents {
            if agent_names.contains(a) {
                continue;
            }
            agent_names.push(a.clone());
        }
        agent_names.sort();
        for agent_name in agent_names {
            let variants = match self.working_agents.get(&agent_name) {
                Some(variants) => variants,
                None => continue,
            };
            // for (agent_name, variants) in &self.working_agents {
            for variant_agent in variants {
                if let Some(behaviors) = variant_agent.get_provides(behavior_name) {
                    for b in behaviors {
                        // if unconditional, add this as a satisfied Offer
                        if b.is_unconditional() {
                            r = r.add_satisfying_offer(Offer::new(&agent_name));
                            continue;
                        }
                        // resolve conditions
                        let resolved_conditions = b
                            .get_conditions()
                            .iter()
                            .map(|c| self.resolve(c))
                            .collect::<Vec<Resolution>>();
                        // if all conditions are satisfied, add this as a satisfied Offer
                        if resolved_conditions.iter().all(|x| x.is_satisfied()) {
                            r = r.add_satisfying_offer(Offer::new_conditional(
                                &agent_name,
                                resolved_conditions,
                            ));
                        // otherwise, add this as an unsatisfied Offer
                        } else {
                            r = r.add_unsatisfying_offer(Offer::new_conditional(
                                &agent_name,
                                resolved_conditions,
                            ));
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
        let mut t = Tracker {
            available_agents: vec![],
            available_superagents: vec![],
            working_agents: HashMap::new(),
        };
        let mut a = Agent::new(String::from("abcd"));
        a.add_provide(Behavior::new_with_conditions(String::from("ba"), vec![]));
        t.add_agent(a);
        t.add_agent(Agent::new(String::from("ijkl")));
        let mut b = Agent::new(String::from("efgh"));
        b.add_provide(Behavior::new_with_conditions(
            String::from("b1"),
            vec![String::from("c1")],
        ));
        b.add_provide(Behavior::new_with_conditions(
            String::from("b2"),
            vec![String::from("c2")],
        ));
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

        assert_eq!(
            t.get_agent_wants(String::from("abcd")),
            HashSet::from([String::from("abcd_w1"), String::from("abcd_w2"),])
        );
        assert_eq!(
            t.get_agent_wants(String::from("efgh")),
            HashSet::from([String::from("efgh_w3"),])
        );
    }

    #[test]
    fn test_simple_resolve() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("a1"))
        );

        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_unsatisfying_offer(Offer::new_conditional("a2", vec!(Resolution::new("b2"))))
        );

        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b2")]));
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new_conditional(
                    "a2",
                    vec!(Resolution::new("b2").add_satisfying_offer(Offer::new("a3")))
                ))
        );
    }

    #[test]
    fn test_resolve_multiple_satisfying() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b1")]));
        let resolve_b1 = Resolution::new("b1")
            .add_satisfying_offer(Offer::new("a1"))
            .add_satisfying_offer(Offer::new("a2"))
            .add_satisfying_offer(Offer::new("a3"));
        assert_eq!(t.resolve("b1"), resolve_b1);
    }

    #[test]
    fn test_resolve_unsatisfied() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2a"), String::from("b2b")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2a")]));

        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_unsatisfying_offer(Offer::new_conditional(
                "a1",
                vec!(
                    Resolution::new("b2a").add_satisfying_offer(Offer::new("a2")),
                    Resolution::new("b2b"),
                )
            ))
        )
    }

    #[test]
    fn test_resolve_deep() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b2").with_conditions(vec![String::from("b3")]),
        ]));
        t.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b3").with_conditions(vec![String::from("b4")]),
        ]));
        t.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b4")]));
        let satisfied_part = Resolution::new("b1").add_satisfying_offer(Offer::new_conditional(
            "a1",
            vec![
                Resolution::new("b2").add_satisfying_offer(Offer::new_conditional(
                    "a2",
                    vec![
                        Resolution::new("b3").add_satisfying_offer(Offer::new_conditional(
                            "a3",
                            vec![Resolution::new("b4").add_satisfying_offer(Offer::new("a4"))],
                        )),
                    ],
                )),
            ],
        ));
        assert_eq!(t.resolve("b1"), satisfied_part);
        t.add_agent(Agent::build("a0").with_provides(vec![
            Behavior::build("b0").with_conditions(vec![String::from("b1"), String::from("b1b")]),
        ]));
        t.add_agent(Agent::build("a1b").with_provides(vec![
            Behavior::build("b1b").with_conditions(vec![String::from("b2b")]),
        ]));
        assert_eq!(
            t.resolve("b0"),
            Resolution::new("b0").add_unsatisfying_offer(Offer::new_conditional(
                "a0",
                vec!(
                    satisfied_part,
                    Resolution::new("b1b").add_unsatisfying_offer(Offer::new_conditional(
                        "a1b",
                        vec!(Resolution::new("b2b"))
                    ))
                )
            ))
        );
    }

    #[test]
    fn test_add_superagent() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b3")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2")
                .with_agent("a3"),
        );
        assert_eq!(t.working_agents.len(), 1);
        let wsa = t.working_agents.get("sa1").unwrap();
        assert_eq!(wsa.len(), 1);
        let all_provides = wsa[0].get_all_provides();
        let mut combined_provides = all_provides.iter().collect::<Vec<&Behavior>>();
        combined_provides.sort();
        assert_eq!(
            combined_provides,
            vec![
                &Behavior::build("b1"),
                &Behavior::build("b2"),
                &Behavior::build("b3"),
            ]
        );

        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b3").with_conditions(vec![String::from("b4")]),
        ]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2")
                .with_agent("a3")
                .with_instance(
                    "i1",
                    "",
                    "i1p",
                    "i1c",
                    vec![Behavior::build("i1p1")],
                    vec![Behavior::build("i1w1")],
                )
                .with_instance("i2", "", "i2p", "i2c", vec![], vec![]),
        );
        assert_eq!(t.working_agents.len(), 2);
        let wsa = t.working_agents.get("i1").unwrap();
        let all_provides = wsa[0].get_all_provides();
        let mut combined_provides = all_provides.iter().collect::<Vec<&Behavior>>();
        combined_provides.sort();
        assert_eq!(
            combined_provides,
            vec![
                &Behavior::build("b1 | i1p"),
                &Behavior::build("b2 | i1p"),
                &Behavior::build("b3 | i1p").with_conditions(vec![String::from("b4 | i1c")]),
                &Behavior::build("i1p1"),
            ]
        );
        let wsa = t.working_agents.get("i2").unwrap();
        let all_provides = wsa[0].get_all_provides();
        let mut combined_provides = all_provides.iter().collect::<Vec<&Behavior>>();
        combined_provides.sort();
        assert_eq!(
            combined_provides,
            vec![
                &Behavior::build("b1 | i2p"),
                &Behavior::build("b2 | i2p"),
                &Behavior::build("b3 | i2p").with_conditions(vec![String::from("b4 | i2c")]),
            ]
        );
    }

    #[test]
    fn test_superagent_resolve() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b3")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2")
                .with_agent("a3"),
        );
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("sa1"))
        );
        assert_eq!(
            t.resolve("b2"),
            Resolution::new("b2").add_satisfying_offer(Offer::new("sa1"))
        );
        assert_eq!(
            t.resolve("b3"),
            Resolution::new("b3").add_satisfying_offer(Offer::new("sa1"))
        );
    }

    #[test]
    fn test_resolve_torture() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        for _ in 0..1_000 {
            assert_eq!(
                t.resolve("b1").to_strings_compressed(false),
                vec!["b1 |-> a1".to_string(), "   |-> a2".to_string(),]
            )
        }
    }

    #[test]
    fn test_superagent_instance_resolve() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b3").with_conditions(vec![String::from("b4")]),
        ]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2")
                .with_agent("a3")
                .with_instance(
                    "i1",
                    "",
                    "i1p",
                    "i1c",
                    vec![Behavior::build("i1p1")],
                    vec![Behavior::build("i1w1")],
                )
                .with_instance("i2", "", "i2p", "i2c", vec![], vec![]),
        );
        // fully internally resolved
        assert_eq!(
            t.resolve("b1 | i1p"),
            Resolution::new("b1 | i1p").add_satisfying_offer(Offer::new("i1"))
        );
        // partially internally resolved but otherwise unresolved
        assert_eq!(
            t.resolve("b3 | i1p"),
            Resolution::new("b3 | i1p").add_unsatisfying_offer(Offer::new_conditional(
                "i1",
                vec![Resolution::new("b4 | i1c")],
            )),
        );
        t.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b4 | i1c")]));
        assert_eq!(
            t.resolve("b3 | i1p"),
            Resolution::new("b3 | i1p").add_satisfying_offer(Offer::new_conditional(
                "i1",
                vec![Resolution::new("b4 | i1c").add_satisfying_offer(Offer::new("a4"))],
            )),
        )
    }
}
