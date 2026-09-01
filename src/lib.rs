pub mod components;
pub mod diagram;
pub mod network_diagram;
pub mod promise_graph;

use components::Agent;
use components::BaseKind;
use components::Instance;
use components::Item;
use components::SuperAgent;
use std::collections::HashMap;
use std::collections::HashSet;

pub mod resolve;
pub mod validate;
use resolve::Offer;
use resolve::Resolution;

#[derive(Debug, Clone)]
pub struct Tracker {
    available_agents: Vec<Agent>,
    available_superagents: Vec<SuperAgent>,
    available_instances: Vec<Instance>,
    working_agents: HashMap<String, Vec<Agent>>,
}

// Need:
// - TODO - schema validation  - ContractCarder
// - TODO ptdiagram?

/// How deep a chain of conditions may go before resolution stops descending.
/// The cycle guard catches a goal that comes back around to itself; this
/// catches a chain that keeps naming something new, which a parameterized
/// condition can do indefinitely.
const MAX_RESOLVE_DEPTH: usize = 64;

impl Tracker {
    pub fn new() -> Tracker {
        Tracker {
            available_agents: vec![],
            available_superagents: vec![],
            available_instances: vec![],
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

    pub fn add_instance(&mut self, i: Instance) {
        for existing in &self.available_instances {
            if existing == &i {
                return;
            }
        }
        let _ = &self.available_instances.push(i.clone());
        self.rebuild();
    }

    pub fn add_item(&mut self, i: Item) {
        match i {
            Item::Agent(a) => self.add_agent(a),
            Item::SuperAgent(sa) => self.add_superagent(sa),
            Item::Instance(i) => self.add_instance(i),
        }
    }

    pub fn rebuild(&mut self) {
        // An agent folded into a collective, and anything an instance is built
        // from, are templates: they describe something rather than being it, so
        // they do not stand as working agents of their own.
        let mut templates: HashSet<String> = HashSet::new();
        for sa in &self.available_superagents {
            templates.extend(sa.get_agent_names());
        }
        for instance in &self.available_instances {
            templates.insert(instance.get_base().name().clone());
        }

        // Collectives first, flattened: an instance built on one needs it
        // whole before it can copy it.
        let collective_names: Vec<String> = self
            .available_superagents
            .iter()
            .map(|sa| sa.get_name().clone())
            .collect();
        let mut collectives: HashMap<String, Agent> = HashMap::new();
        for name in collective_names {
            if collectives.contains_key(&name) {
                continue;
            }
            if let Some(agent) = self.flatten_collective(&name, &mut HashSet::new()) {
                collectives.insert(name, agent);
            }
        }

        let mut new_working_agents: HashMap<String, Vec<Agent>> = HashMap::new();

        for (name, collective) in &collectives {
            if templates.contains(name) {
                continue;
            }
            let e = new_working_agents
                .entry(name.clone())
                .or_insert(vec![collective.clone()]);
            e[0].merge(collective);
        }

        for instance in &self.available_instances {
            let base = self.find_base(instance);
            let instance_agent = instance.materialize(&base);
            let e = new_working_agents
                .entry(instance_agent.get_name().clone())
                .or_insert(vec![instance_agent.clone()]);
            e[0].merge(&instance_agent);
        }

        for a in &self.available_agents {
            if templates.contains(a.get_name()) {
                continue;
            }
            let e = new_working_agents
                .entry(a.get_name().clone())
                .or_insert(vec![a.clone()]);
            e[0].merge(&a);
        }

        self.working_agents = new_working_agents;
    }

    /// One collective folded into a single agent, its members merged in and
    /// its internally-met conditions reduced away.
    ///
    /// A member that names another collective is folded in whole first, so a
    /// collective built from collectives promises everything its nesting
    /// reaches. `visiting` carries the collectives already being folded on the
    /// way down: one that comes back around to itself contributes nothing the
    /// second time through rather than descending forever.
    ///
    /// Returns `None` when nothing declares a collective by that name.
    fn flatten_collective(&self, name: &str, visiting: &mut HashSet<String>) -> Option<Agent> {
        let members: Vec<String> = self
            .available_superagents
            .iter()
            .filter(|sa| sa.get_name() == name)
            .flat_map(|sa| sa.get_agent_names())
            .collect();
        if !self
            .available_superagents
            .iter()
            .any(|sa| sa.get_name() == name)
        {
            return None;
        }
        if !visiting.insert(name.to_string()) {
            return Some(Agent::new(name.to_string()));
        }

        let mut stub = Agent::new(name.to_string());
        for member in members {
            // A collective member first, matching how an unqualified instance
            // base is looked up; a plain agent otherwise.
            match self.flatten_collective(&member, visiting) {
                Some(inner) => stub.merge(&inner),
                None => self
                    .available_agents
                    .iter()
                    .filter(|a| a.get_name() == &member)
                    .for_each(|a| stub.merge(a)),
            }
        }
        // reduce its behaviors to those that are not internally handled
        stub.reduce();

        visiting.remove(name);
        Some(stub)
    }

    /// What an instance is built from.
    ///
    /// A base that names nothing yields an empty agent, so the instance still
    /// carries whatever it declares itself and the rest of the contract keeps
    /// loading; [`Tracker::dangling_instance_bases`] is what reports it.
    fn find_base(&self, instance: &Instance) -> Agent {
        let base = instance.get_base();
        let name = base.name();
        let collective = || self.flatten_collective(name, &mut HashSet::new());
        let plain = || {
            self.available_agents
                .iter()
                .find(|a| a.get_name() == name)
                .cloned()
        };
        match base.kind() {
            Some(BaseKind::SuperAgent) => collective(),
            Some(BaseKind::Agent) => plain(),
            // Unqualified: a collective first, then a plain agent.
            None => collective().or_else(plain),
        }
        .unwrap_or_else(|| Agent::new(name.clone()))
    }

    /// Working agents whose wants are still parameterized, as
    /// `(agent, want)` pairs.
    ///
    /// Restriction A: resolution starts from a concrete goal, so a want has to
    /// name one. A want may carry a variable in the document that declares it,
    /// as long as an instance's bindings fill it in — which is why this is
    /// answered here, with every document in play, rather than per contract.
    pub fn non_ground_wants(&self) -> Vec<(String, String)> {
        let mut ret: Vec<(String, String)> = vec![];
        for (agent_name, variants) in &self.working_agents {
            for variant in variants {
                for want in variant.wants() {
                    if !want.get_name_pattern().is_ground() {
                        ret.push((agent_name.clone(), want.get_name().clone()));
                    }
                }
            }
        }
        ret.sort();
        ret.dedup();
        ret
    }

    /// Instances whose `base` names nothing loaded here, as
    /// `(instance, base)` pairs.
    ///
    /// This needs every document in play to answer, which is why it lives on
    /// the tracker rather than in the per-contract validation pass: a base may
    /// perfectly well be declared in a different file.
    pub fn dangling_instance_bases(&self) -> Vec<(String, String)> {
        let mut ret: Vec<(String, String)> = self
            .available_instances
            .iter()
            .filter(|i| {
                let name = i.get_base().name();
                let is_collective = self
                    .available_superagents
                    .iter()
                    .any(|sa| sa.get_name() == name);
                let is_agent = self.available_agents.iter().any(|a| a.get_name() == name);
                match i.get_base().kind() {
                    Some(BaseKind::SuperAgent) => !is_collective,
                    Some(BaseKind::Agent) => !is_agent,
                    None => !is_collective && !is_agent,
                }
            })
            .map(|i| (i.get_name().clone(), i.get_base().to_string()))
            .collect();
        ret.sort();
        ret
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

    /// Is `behavior_name` a concrete behavior something here could answer, ask
    /// for, or depend on — matching a plain declaration, or a pattern covering it?
    ///
    /// [`Tracker::has_behavior`] asks the narrower question of whether that
    /// exact text was written down anywhere. Once names can be parameterized
    /// the two answers diverge, and anything guarding a call to
    /// [`Tracker::resolve`] wants this one.
    pub fn has_ground_behavior(&self, behavior_name: &str) -> bool {
        self.working_agents.iter().any(|(_, variants)| {
            variants
                .iter()
                .any(|a| a.has_ground_behavior(behavior_name))
        })
    }

    /// The parameterized names in play, as written. Each stands for a family of
    /// behaviors rather than naming one, so none of these can be resolved.
    pub fn get_behavior_patterns(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        for (_, variants) in &self.working_agents {
            for variant_agent in variants {
                ret.extend(variant_agent.get_behavior_patterns());
            }
        }
        ret
    }

    /// The concrete behaviors this agent promises.
    pub fn get_agent_provides(&self, agent_name: &str) -> Option<HashSet<String>> {
        let agents = self.working_agents.get(agent_name)?;
        let mut ret: HashSet<String> = HashSet::new();
        for agent in agents {
            for behavior in agent.get_all_provides() {
                if behavior.get_name_pattern().is_ground() {
                    ret.insert(behavior.get_name().clone());
                }
            }
        }
        Some(ret)
    }

    /// The parameterized promises this agent makes, as written.
    pub fn get_agent_provide_patterns(&self, agent_name: &str) -> Option<HashSet<String>> {
        let agents = self.working_agents.get(agent_name)?;
        let mut ret: HashSet<String> = HashSet::new();
        for agent in agents {
            for behavior in agent.get_all_provides() {
                if !behavior.get_name_pattern().is_ground() {
                    ret.insert(behavior.get_name().clone());
                }
            }
        }
        Some(ret)
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
            return ret;
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
        let mut in_progress: Vec<String> = vec![];
        let mut settled: HashMap<String, Resolution> = HashMap::new();
        self.resolve_goal(behavior_name, &mut in_progress, &mut settled)
            .0
    }

    /// Resolve one concrete goal.
    ///
    /// The second return value says whether the answer stands on its own. It is
    /// false when the subtree was cut short by the cycle guard or the depth cap,
    /// because such an answer depends on the path it was reached by and must not
    /// be reused for the same goal elsewhere.
    fn resolve_goal(
        &self,
        goal: &str,
        in_progress: &mut Vec<String>,
        settled: &mut HashMap<String, Resolution>,
    ) -> (Resolution, bool) {
        // A promise that depends on itself, however far around, keeps nothing.
        if in_progress.iter().any(|g| g == goal) {
            return (Resolution::new(goal), false);
        }
        if in_progress.len() >= MAX_RESOLVE_DEPTH {
            return (Resolution::new(goal), false);
        }
        if let Some(known) = settled.get(goal) {
            return (known.clone(), true);
        }

        in_progress.push(goal.to_string());
        let mut r = Resolution::new(goal);
        let mut self_contained = true;

        let mut agent_names: Vec<&String> = self.working_agents.keys().collect();
        agent_names.sort();
        for agent_name in agent_names {
            let Some(variants) = self.working_agents.get(agent_name) else {
                continue;
            };
            for variant_agent in variants {
                for (behavior, bindings) in variant_agent.get_matching_provides(goal) {
                    // if unconditional, add this as a satisfied Offer
                    if behavior.is_unconditional() {
                        r = r.add_satisfying_offer(Offer::new(agent_name));
                        continue;
                    }
                    // resolve conditions, with whatever the goal bound
                    // substituted into them
                    let mut resolved_conditions = Vec::new();
                    for condition in behavior.get_condition_patterns() {
                        let sub_goal = condition.substitute(&bindings);
                        let (resolved, stands_alone) =
                            self.resolve_goal(sub_goal.source(), in_progress, settled);
                        self_contained &= stands_alone;
                        resolved_conditions.push(resolved);
                    }
                    // if all conditions are satisfied, add this as a satisfied Offer
                    if resolved_conditions.iter().all(|x| x.is_satisfied()) {
                        r = r.add_satisfying_offer(Offer::new_conditional(
                            agent_name,
                            resolved_conditions,
                        ));
                    // otherwise, add this as an unsatisfied Offer
                    } else {
                        r = r.add_unsatisfying_offer(Offer::new_conditional(
                            agent_name,
                            resolved_conditions,
                        ));
                    }
                }
            }
        }

        in_progress.pop();
        if self_contained {
            settled.insert(goal.to_string(), r.clone());
        }
        (r, self_contained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use components::Behavior;
    use components::Instance;

    #[test]
    fn simple_adds() {
        let mut t = Tracker {
            available_agents: vec![],
            available_superagents: vec![],
            available_instances: vec![],
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

        assert_eq!(t.get_agent_provides("nope"), None);
        assert_eq!(
            t.get_agent_provides("abcd"),
            Some(
                HashSet::from(["ba"])
                    .iter()
                    .map(|x| x.to_string())
                    .collect()
            )
        );
        assert_eq!(
            t.get_agent_provides("efgh"),
            Some(
                HashSet::from(["b1", "b2"])
                    .iter()
                    .map(|x| x.to_string())
                    .collect()
            )
        );
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
                .with_agent("a3"),
        );
        t.add_instance(
            Instance::new("i1", "SuperAgent/sa1")
                .with_provides(vec![Behavior::build("i1p1")])
                .with_wants(vec![Behavior::build("i1w1")]),
        );
        t.add_instance(Instance::new("i2", "SuperAgent/sa1"));
        // The collective is a template once something is built from it, so
        // only the copies stand as working agents.
        assert_eq!(t.working_agents.len(), 2);
        let wsa = t.working_agents.get("i1").unwrap();
        let all_provides = wsa[0].get_all_provides();
        let mut combined_provides = all_provides.iter().collect::<Vec<&Behavior>>();
        combined_provides.sort();
        assert_eq!(
            combined_provides,
            vec![
                &Behavior::build("b1"),
                &Behavior::build("b2"),
                &Behavior::build("b3").with_conditions(vec![String::from("b4")]),
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
                &Behavior::build("b1"),
                &Behavior::build("b2"),
                &Behavior::build("b3").with_conditions(vec![String::from("b4")]),
            ]
        );

        assert_eq!(
            t.get_agent_provides("i1"),
            Some(
                HashSet::from(["i1p1", "b1", "b2", "b3"])
                    .iter()
                    .map(|x| x.to_string())
                    .collect()
            )
        );

        // Instance-specific wants land on that instance and no other.
        assert_eq!(
            t.get_agent_wants(String::from("i1")),
            HashSet::from([String::from("i1w1")])
        );
        assert_eq!(t.get_agent_wants(String::from("i2")), HashSet::new());
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
                .with_agent("a3"),
        );
        t.add_instance(
            Instance::new("i1", "SuperAgent/sa1")
                .with_provides(vec![Behavior::build("i1p1")])
                .with_wants(vec![Behavior::build("i1w1")]),
        );
        t.add_instance(Instance::new("i2", "SuperAgent/sa1"));
        // Unbound copies share the collective's behavior names verbatim, so every
        // copy offers every behavior and the offers are indistinguishable.
        // Bindings are what tell them apart; see the test below.

        // fully internally resolved, by both instances
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("i1"))
                .add_satisfying_offer(Offer::new("i2"))
        );
        // partially internally resolved but otherwise unresolved
        assert_eq!(
            t.resolve("b3"),
            Resolution::new("b3")
                .add_unsatisfying_offer(Offer::new_conditional("i1", vec![Resolution::new("b4")],))
                .add_unsatisfying_offer(Offer::new_conditional("i2", vec![Resolution::new("b4")],)),
        );
        // one outside provider satisfies the same condition for both instances
        t.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b4")]));
        assert_eq!(
            t.resolve("b3"),
            Resolution::new("b3")
                .add_satisfying_offer(Offer::new_conditional(
                    "i1",
                    vec![Resolution::new("b4").add_satisfying_offer(Offer::new("a4"))],
                ))
                .add_satisfying_offer(Offer::new_conditional(
                    "i2",
                    vec![Resolution::new("b4").add_satisfying_offer(Offer::new("a4"))],
                )),
        )
    }

    /// A collective built from another collective promises what the nesting
    /// reaches: sa2's members are sa1 (a1 + a2) and a3, so the chain
    /// w <- wa1 <- wa2 closes inside sa2.
    #[test]
    fn test_nested_superagent_resolve() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("w")]));
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("wa1").with_conditions(vec![String::from("wa2")]),
        ]));
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("wa2")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("sa1")
                .with_agent("a3"),
        );

        // Only the outermost collective stands as a working agent: its members,
        // sa1 among them, are templates.
        let mut working = t.get_working_agent_names();
        working.sort();
        assert_eq!(working, vec!["sa2", "w1"]);

        assert_eq!(
            t.resolve("w"),
            Resolution::new("w").add_satisfying_offer(Offer::new("sa2"))
        );
    }

    /// The same nesting, with every document arriving before what it names.
    /// Each add rebuilds from scratch, so the order documents load in does not
    /// change what the collectives come out as.
    #[test]
    fn test_nested_superagent_built_in_any_order() {
        let mut t = Tracker::new();
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("sa1")
                .with_agent("a3"),
        );
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("wa2")]));
        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("wa1").with_conditions(vec![String::from("wa2")]),
        ]));
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("w")]));

        assert_eq!(
            t.resolve("w"),
            Resolution::new("w").add_satisfying_offer(Offer::new("sa2"))
        );
    }

    /// Nesting three deep, with the condition met a level out from where it is
    /// named: sa1 leaves wa2 open, sa2 leaves wa3 open, and sa3 closes it.
    #[test]
    fn test_nested_superagent_three_deep() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("wa1").with_conditions(vec![String::from("wa2")]),
        ]));
        t.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("wa2").with_conditions(vec![String::from("wa3")]),
        ]));
        t.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("wa3")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("sa1")
                .with_agent("a3"),
        );
        t.add_superagent(
            SuperAgent::new(String::from("sa3"))
                .with_agent("sa2")
                .with_agent("a4"),
        );

        assert_eq!(t.get_working_agent_names(), vec!["sa3"]);
        assert_eq!(
            t.resolve("w"),
            Resolution::new("w").add_satisfying_offer(Offer::new("sa3"))
        );
    }

    /// A collective whose nesting comes back around to itself still builds:
    /// the second pass through contributes nothing rather than descending
    /// forever.
    #[test]
    fn test_nested_superagent_cycle_terminates() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("wa1")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("sa2"),
        );
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("a2")
                .with_agent("sa1"),
        );

        // Each names the other, so both are templates and neither stands as a
        // working agent; what matters here is that the rebuild terminates.
        assert!(t.get_working_agent_names().is_empty());
        assert_eq!(t.resolve("w"), Resolution::new("w"));
    }

    /// A copy of a nested collective carries the whole nesting, not just the
    /// members named directly.
    #[test]
    fn test_instance_of_a_nested_superagent() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("wa1")]));
        t.add_agent(Agent::build("a3").with_wants(vec![Behavior::build("w")]));
        t.add_superagent(SuperAgent::new(String::from("sa1")).with_agent("a1"));
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("sa1")
                .with_agent("a2"),
        );
        t.add_instance(Instance::new("i1", "SuperAgent/sa2"));

        assert_eq!(
            t.resolve("w"),
            Resolution::new("w").add_satisfying_offer(Offer::new("i1"))
        );
    }

    #[test]
    fn test_resolve_parameterized_promise() {
        let mut t = Tracker::new();
        // One host, promising execution to whoever asks, on terms that name
        // the asker.
        t.add_agent(Agent::build("host").with_provides(vec![
            Behavior::build("process-execution/{{process}}")
                .with_conditions(vec![String::from("binary-installed/{{process}}")]),
        ]));
        t.add_agent(Agent::build("p1").with_wants(vec![Behavior::build("process-execution/p1")]));
        t.add_agent(Agent::build("p2").with_wants(vec![Behavior::build("process-execution/p2")]));
        t.add_agent(
            Agent::build("packaging").with_provides(vec![Behavior::build("binary-installed/p1")]),
        );

        // p1's binary is installed, so the host can keep its promise to p1
        assert_eq!(
            t.resolve("process-execution/p1"),
            Resolution::new("process-execution/p1").add_satisfying_offer(Offer::new_conditional(
                "host",
                vec![Resolution::new("binary-installed/p1")
                    .add_satisfying_offer(Offer::new("packaging"))],
            ))
        );
        // p2's is not — and that is now sayable separately, which is the whole
        // point of the exercise
        assert_eq!(
            t.resolve("process-execution/p2"),
            Resolution::new("process-execution/p2").add_unsatisfying_offer(Offer::new_conditional(
                "host",
                vec![Resolution::new("binary-installed/p2")]
            ))
        );
    }

    #[test]
    fn test_resolve_pattern_and_ground_provider_both_offer() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("generic").with_provides(vec![Behavior::build("run/{{p}}")]));
        t.add_agent(Agent::build("special").with_provides(vec![Behavior::build("run/x")]));

        // No specificity ordering: a wanter sees everyone who can help.
        assert_eq!(
            t.resolve("run/x"),
            Resolution::new("run/x")
                .add_satisfying_offer(Offer::new("generic"))
                .add_satisfying_offer(Offer::new("special"))
        );
        // The ground provider only answers its own name.
        assert_eq!(
            t.resolve("run/y"),
            Resolution::new("run/y").add_satisfying_offer(Offer::new("generic"))
        );
    }

    #[test]
    fn test_resolve_terminates_on_a_cycle() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("b").with_provides(vec![
            Behavior::build("b2").with_conditions(vec![String::from("b1")]),
        ]));

        // Coming back around to a goal already being resolved yields nothing,
        // so neither promise can be kept and the walk ends.
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_unsatisfying_offer(Offer::new_conditional(
                "a",
                vec![
                    Resolution::new("b2").add_unsatisfying_offer(Offer::new_conditional(
                        "b",
                        vec![Resolution::new("b1")],
                    ))
                ],
            ))
        );
    }

    #[test]
    fn test_resolve_terminates_on_an_ever_growing_goal() {
        let mut t = Tracker::new();
        // `{{x}}` can be kept only if `{{x}}x` can, which names something one
        // character longer every time. The goal never repeats, so the cycle
        // guard cannot help and the depth cap has to.
        t.add_agent(Agent::build("grower").with_provides(vec![
            Behavior::build("{{x}}").with_conditions(vec![String::from("{{x}}x")]),
        ]));

        let r = t.resolve("a");
        assert!(!r.is_satisfied());
    }

    #[test]
    fn test_resolve_reuses_a_settled_goal() {
        let mut t = Tracker::new();
        // `shared` is reached through both conditions of `top`; the answer is
        // the same either way.
        t.add_agent(Agent::build("t").with_provides(vec![
            Behavior::build("top").with_conditions(vec![String::from("l"), String::from("r")]),
        ]));
        t.add_agent(Agent::build("l").with_provides(vec![
            Behavior::build("l").with_conditions(vec![String::from("shared")]),
        ]));
        t.add_agent(Agent::build("r").with_provides(vec![
            Behavior::build("r").with_conditions(vec![String::from("shared")]),
        ]));
        t.add_agent(Agent::build("s").with_provides(vec![Behavior::build("shared")]));

        let shared = Resolution::new("shared").add_satisfying_offer(Offer::new("s"));
        assert_eq!(
            t.resolve("top"),
            Resolution::new("top").add_satisfying_offer(Offer::new_conditional(
                "t",
                vec![
                    Resolution::new("l")
                        .add_satisfying_offer(Offer::new_conditional("l", vec![shared.clone()])),
                    Resolution::new("r")
                        .add_satisfying_offer(Offer::new_conditional("r", vec![shared])),
                ],
            ))
        );
    }

    #[test]
    fn test_instances_are_told_apart_by_their_bindings() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("api").with_provides(vec![
            Behavior::build("kube-api/{{env}}").with_conditions(vec![String::from("etcd/{{env}}")]),
        ]));
        t.add_instance(Instance::new("prod", "Agent/api").with_binding("env", "prod"));
        t.add_instance(Instance::new("staging", "Agent/api").with_binding("env", "staging"));
        t.add_agent(Agent::build("etcd-prod").with_provides(vec![Behavior::build("etcd/prod")]));

        // The base is a template, so only its copies stand as agents.
        assert_eq!(
            t.get_working_agent_names(),
            vec!["etcd-prod", "prod", "staging"]
        );
        // prod's dependency is met and staging's is not — two separate
        // questions, which is what instancing is for.
        assert_eq!(
            t.resolve("kube-api/prod"),
            Resolution::new("kube-api/prod").add_satisfying_offer(Offer::new_conditional(
                "prod",
                vec![Resolution::new("etcd/prod").add_satisfying_offer(Offer::new("etcd-prod"))],
            ))
        );
        assert_eq!(
            t.resolve("kube-api/staging"),
            Resolution::new("kube-api/staging").add_unsatisfying_offer(Offer::new_conditional(
                "staging",
                vec![Resolution::new("etcd/staging")],
            ))
        );
    }

    #[test]
    fn test_instance_may_bind_only_part_of_a_name() {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("api").with_provides(vec![Behavior::build("api/{{env}}/{{tenant}}")]),
        );
        t.add_instance(Instance::new("prod", "api").with_binding("env", "prod"));

        // The instance fixes env; the wanter still names the tenant.
        assert_eq!(
            t.resolve("api/prod/acme"),
            Resolution::new("api/prod/acme").add_satisfying_offer(Offer::new("prod"))
        );
        assert_eq!(
            t.resolve("api/staging/acme"),
            Resolution::new("api/staging/acme")
        );
    }

    #[test]
    fn test_an_uninstantiated_base_stands_on_its_own() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("api").with_provides(vec![Behavior::build("thing")]));
        // Nothing is built from it, so it is a component rather than a template.
        assert_eq!(t.get_working_agent_names(), vec!["api"]);

        t.add_instance(Instance::new("copy", "api"));
        assert_eq!(t.get_working_agent_names(), vec!["copy"]);
    }

    #[test]
    fn test_dangling_and_open_wants_are_reported() {
        let mut t = Tracker::new();
        t.add_instance(Instance::new("orphan", "SuperAgent/nowhere"));
        t.add_agent(Agent::build("w").with_wants(vec![Behavior::build("thing/{{v}}")]));

        assert_eq!(
            t.dangling_instance_bases(),
            vec![("orphan".to_string(), "SuperAgent/nowhere".to_string())]
        );
        assert_eq!(
            t.non_ground_wants(),
            vec![("w".to_string(), "thing/{{v}}".to_string())]
        );

        // Binding it from an instance settles it.
        let mut t = Tracker::new();
        t.add_agent(Agent::build("base").with_wants(vec![Behavior::build("thing/{{v}}")]));
        t.add_instance(Instance::new("bound", "base").with_binding("v", "x"));
        assert!(t.non_ground_wants().is_empty());
        assert_eq!(
            t.get_agent_wants(String::from("bound")),
            HashSet::from([String::from("thing/x")])
        );
    }

    #[test]
    fn test_instance_of_a_plain_agent_from_yaml() {
        let mut t = Tracker::new();
        for document in serde_yaml::Deserializer::from_str(
            "kind: Agent
name: host
provides:
  - name: run/{{env}}
---
kind: Instance
name: host-prod
base: Agent/host
bindings:
  env: prod
",
        ) {
            t.add_item(<Item as serde::Deserialize>::deserialize(document).unwrap());
        }
        assert_eq!(t.get_working_agent_names(), vec!["host-prod"]);
        assert_eq!(
            t.resolve("run/prod"),
            Resolution::new("run/prod").add_satisfying_offer(Offer::new("host-prod"))
        );
    }
}
