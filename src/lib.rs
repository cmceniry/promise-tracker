pub mod components;
pub mod diagram;
pub mod fold;
pub mod marks;
pub mod network_diagram;
pub mod promise_graph;
pub mod query;
pub mod trace;

use components::Agent;
use components::BaseKind;
use components::Bindings;
use components::Behavior;
use components::Instance;
use components::Item;
use components::Pattern;
use components::SuperAgent;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
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
    /// Whether a collective stands as one merged agent. See
    /// [`Tracker::set_fold_collectives`].
    fold_collectives: bool,
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
            fold_collectives: true,
        }
    }

    /// Whether collectives are folded into one working agent each. On by
    /// default.
    pub fn folds_collectives(&self) -> bool {
        self.fold_collectives
    }

    /// Choose how a collective (SuperAgent) stands among the working agents,
    /// and rebuild if that changes anything.
    ///
    /// Folded (the default): the collective is one working agent with its
    /// members merged in and its internally-met conditions reduced away; the
    /// members are templates and do not stand on their own.
    ///
    /// Unfolded: the collective is not a working agent. Every agent it
    /// reaches, through any nesting, stands as its own working agent, so a
    /// promise one member makes to another is an ordinary promise between two
    /// agents and a want resolves against the members one by one.
    ///
    /// Instances are the exception. An instance is a copy of its base, and a
    /// copy of a collective is a copy of the whole folded collective, so an
    /// instance built on one is materialized exactly as in folded mode. A
    /// collective that exists only to be copied (an instance base, or one
    /// nested inside one) is a template, and so is an agent that nothing but
    /// such collectives reaches: unfolding it as well would put its promises
    /// on the board twice, once as itself and once inside every copy.
    ///
    /// Unfolding never loses a promise, though. An agent that any other
    /// collective reaches, through any nesting, stands on its own even when
    /// a copied collective also holds it, because folded it would stand
    /// inside that other collective. The same goes for an agent that is
    /// itself an instance base: folded into a collective it still counts
    /// there, so unfolded it stands.
    ///
    /// Collectives that contain one another (see
    /// [`Tracker::membership_cycles`]) leave nothing standing when folded;
    /// unfolded, the agents the loop reaches stand on their own.
    pub fn set_fold_collectives(&mut self, fold: bool) {
        if self.fold_collectives != fold {
            self.fold_collectives = fold;
            self.rebuild();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.working_agents.len() == 0
    }

    pub fn add_agent(&mut self, a: Agent) {
        if self.push_agent(a) {
            self.rebuild();
        }
    }

    pub fn add_superagent(&mut self, sa: SuperAgent) {
        if self.push_superagent(sa) {
            self.rebuild();
        }
    }

    pub fn add_instance(&mut self, i: Instance) {
        if self.push_instance(i) {
            self.rebuild();
        }
    }

    pub fn add_item(&mut self, i: Item) {
        if self.push_item(i) {
            self.rebuild();
        }
    }

    /// Add many items and rebuild once at the end. Adding them one at a time
    /// rebuilds after each, which is quadratic in the size of the contracts.
    pub fn add_items<I: IntoIterator<Item = Item>>(&mut self, items: I) {
        let mut changed = false;
        for i in items {
            changed |= self.push_item(i);
        }
        if changed {
            self.rebuild();
        }
    }

    /// Record an item without rebuilding; false when it was already there.
    fn push_item(&mut self, i: Item) -> bool {
        match i {
            Item::Agent(a) => self.push_agent(a),
            Item::SuperAgent(sa) => self.push_superagent(sa),
            Item::Instance(i) => self.push_instance(i),
        }
    }

    fn push_agent(&mut self, a: Agent) -> bool {
        if self.available_agents.contains(&a) {
            return false;
        }
        self.available_agents.push(a);
        true
    }

    fn push_superagent(&mut self, sa: SuperAgent) -> bool {
        if self.available_superagents.contains(&sa) {
            return false;
        }
        self.available_superagents.push(sa);
        true
    }

    fn push_instance(&mut self, i: Instance) -> bool {
        if self.available_instances.contains(&i) {
            return false;
        }
        self.available_instances.push(i);
        true
    }

    pub fn rebuild(&mut self) {
        // An agent folded into a collective, and anything an instance is built
        // from, are templates: they describe something rather than being it, so
        // they do not stand as working agents of their own.
        let mut templates: HashSet<String> = HashSet::new();
        for instance in &self.available_instances {
            templates.insert(instance.get_base().name().clone());
        }
        if self.fold_collectives {
            for sa in &self.available_superagents {
                templates.extend(sa.get_agent_names());
            }
        } else {
            // Only what nothing but a copy reaches stays a template; see
            // `set_fold_collectives`. First the collectives that exist only
            // to be copied: instance bases, and what is nested inside them.
            let mut copied: HashSet<String> = HashSet::new();
            let mut queue: Vec<String> = self
                .available_instances
                .iter()
                .filter_map(|i| self.collective_base(i))
                .map(str::to_string)
                .collect();
            while let Some(name) = queue.pop() {
                if !copied.insert(name.clone()) {
                    continue;
                }
                for sa in self
                    .available_superagents
                    .iter()
                    .filter(|sa| sa.get_name() == &name)
                {
                    for member in sa.get_agent_names() {
                        if self
                            .available_superagents
                            .iter()
                            .any(|other| other.get_name() == &member)
                        {
                            queue.push(member);
                        }
                    }
                }
            }
            for name in &copied {
                templates.extend(self.collective_members(name).unwrap_or_default());
            }
            // Then everything any other collective reaches stands, copied or
            // not, since folded it would stand inside that collective.
            let mut standing: HashSet<String> = HashSet::new();
            for sa in &self.available_superagents {
                if !copied.contains(sa.get_name()) {
                    standing.extend(self.collective_members(sa.get_name()).unwrap_or_default());
                }
            }
            templates.retain(|t| !standing.contains(t));
        }

        // Collectives first, flattened: an instance built on one needs it
        // whole before it can copy it. Unfolded, none of them stands, and an
        // instance flattens its own base in `find_base`.
        let collective_names: Vec<String> = if self.fold_collectives {
            self.available_superagents
                .iter()
                .map(|sa| sa.get_name().clone())
                .collect()
        } else {
            vec![]
        };
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
            let a = a.bind_self();
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
                    // `{{!self}}` is the member, not the collective it is
                    // folded into, so the text reads the same unfolded.
                    .for_each(|a| stub.merge(&a.bind_self())),
            }
        }
        // reduce its behaviors to those that are not internally handled
        stub.reduce();

        visiting.remove(name);
        Some(stub)
    }

    /// The collective an instance is built on, when its base names one: a
    /// base qualified as a SuperAgent, or an unqualified base that a
    /// collective is declared under (the lookup [`Tracker::find_base`] makes).
    fn collective_base<'a>(&self, instance: &'a Instance) -> Option<&'a str> {
        let base = instance.get_base();
        let name = base.name();
        let declared = self
            .available_superagents
            .iter()
            .any(|sa| sa.get_name() == name);
        match base.kind() {
            Some(BaseKind::SuperAgent) | None if declared => Some(name.as_str()),
            _ => None,
        }
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

    /// Collectives that contain one another, as the loop of names that comes
    /// back around to itself.
    ///
    /// A collective folded into another is a template: it describes something
    /// rather than being it, so it does not stand as a working agent of its
    /// own. When two contain each other, each makes a template of the other and
    /// neither is left standing — nor is anything they hold. The contract
    /// parses, every document loads, and the agents simply are not there.
    ///
    /// Rotated and deduplicated the way [`Tracker::dependency_cycles`] is, so
    /// `["sa1", "sa2"]` is sa1 containing sa2 containing sa1. A collective that
    /// lists itself comes back as a loop of one.
    pub fn membership_cycles(&self) -> Vec<Vec<String>> {
        let mut names: Vec<String> = self
            .available_superagents
            .iter()
            .map(|sa| sa.get_name().clone())
            .collect();
        names.sort();
        names.dedup();

        let mut found: BTreeSet<Vec<String>> = BTreeSet::new();
        let mut clean: HashSet<String> = HashSet::new();
        for name in &names {
            let mut visiting: Vec<String> = vec![];
            self.walk_membership(name, &mut visiting, &mut clean, &mut found);
        }
        found.into_iter().collect()
    }

    /// The descent [`Tracker::flatten_collective`] makes over `agents:` lists,
    /// keeping the loops rather than the promises.
    ///
    /// Returns whether the collective's members were walked in full. The
    /// membership graph is finite — its nodes are the collectives declared —
    /// so the guard on the path down is the only bound needed.
    fn walk_membership(
        &self,
        name: &str,
        visiting: &mut Vec<String>,
        clean: &mut HashSet<String>,
        found: &mut BTreeSet<Vec<String>>,
    ) -> bool {
        if let Some(at) = visiting.iter().position(|n| n == name) {
            found.insert(rotate_to_first(&visiting[at..]));
            return false;
        }
        if clean.contains(name) {
            return true;
        }

        let members: Vec<String> = self
            .available_superagents
            .iter()
            .filter(|sa| sa.get_name() == name)
            .flat_map(|sa| sa.get_agent_names())
            .collect();

        visiting.push(name.to_string());
        let mut complete = true;
        for member in members {
            // Only a member that is itself a collective is an edge here; a
            // plain agent is where the nesting ends.
            if self
                .available_superagents
                .iter()
                .any(|sa| sa.get_name() == &member)
            {
                complete &= self.walk_membership(&member, visiting, clean, found);
            }
        }
        visiting.pop();

        if complete {
            clean.insert(name.to_string());
        }
        complete
    }

    /// Declared (`forSome`) variables that can be anything: the only answer
    /// is a promise that holds for any value of them, and nothing could name
    /// one — whoever asks. Sorted, each once.
    ///
    /// The search runs on each condition using the variable as written, the
    /// promise name's variables left open too, so a value something names
    /// for any asker counts. A variable nothing answers at all is not listed:
    /// that is an unsatisfied condition, not an invalid one.
    ///
    /// Like [`Tracker::non_ground_wants`], this needs every document in
    /// play to answer.
    pub fn unnamable_variables(&self) -> Vec<UnnamableVariable> {
        // Each declaration as its author wrote it, under the document that
        // wrote it: an agent (a collective's member or an instance's base
        // included) or an instance's own promises. Not the working agents,
        // where folding puts a member's promise under the collective and
        // hoisting renames what an inner promise declared. The search runs
        // against the working agents, as resolution does.
        let mut declarations: Vec<(String, Behavior)> = vec![];
        for agent in &self.available_agents {
            for promise in agent.bind_self().provides() {
                declarations.push((agent.get_name().clone(), promise.clone()));
            }
        }
        for instance in &self.available_instances {
            for promise in instance.get_provides() {
                declarations.push((
                    instance.get_name().clone(),
                    promise.bind_self(instance.get_name()),
                ));
            }
        }
        let mut ret: Vec<UnnamableVariable> = vec![];
        let mut memo: HashMap<String, Proposals> = HashMap::new();
        for (agent_name, promise) in &declarations {
            for variable in promise.get_for_some() {
                let mut named = false;
                let mut holders: Vec<HoldsForAny> = vec![];
                for condition in promise.get_condition_patterns() {
                    if !condition.vars().contains(variable.as_str()) {
                        continue;
                    }
                    let (found, _) = self.search(condition, &mut vec![], &mut memo);
                    named |= found.names(variable);
                    for holder in found.holding_for_any(variable) {
                        if !holders.contains(holder) {
                            holders.push(holder.clone());
                        }
                    }
                }
                if named || holders.is_empty() {
                    continue;
                }
                let found = UnnamableVariable {
                    agent: agent_name.clone(),
                    promise: promise.get_name().clone(),
                    variable: variable.clone(),
                    holders,
                };
                if !ret.contains(&found) {
                    ret.push(found);
                }
            }
        }
        ret.sort();
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

    /// The agents a collective is built from, flattened through any
    /// collectives among them, in declaration order without repeats. None
    /// when nothing declares a collective by that name.
    pub fn collective_members(&self, name: &str) -> Option<Vec<String>> {
        if !self
            .available_superagents
            .iter()
            .any(|sa| sa.get_name() == name)
        {
            return None;
        }
        let mut out: Vec<String> = vec![];
        let mut visiting: HashSet<String> = HashSet::new();
        self.collect_members(name, &mut visiting, &mut out);
        Some(out)
    }

    /// The collectives that stand in for `member` when collectives are
    /// folded: every collective reaching it, through any nesting, that is not
    /// itself folded into another collective or copied by an instance. Sorted;
    /// empty when nothing folds it. Answered the same whether or not the
    /// tracker is folded at the moment, so a view can ask before switching.
    pub fn folding_collectives(&self, member: &str) -> Vec<String> {
        let nested: HashSet<String> = self
            .available_superagents
            .iter()
            .flat_map(|sa| sa.get_agent_names())
            .collect();
        let copied: HashSet<&str> = self
            .available_instances
            .iter()
            .map(|i| i.get_base().name().as_str())
            .collect();
        let mut ret: Vec<String> = self
            .available_superagents
            .iter()
            .map(|sa| sa.get_name())
            .filter(|name| !nested.contains(*name) && !copied.contains(name.as_str()))
            .filter(|name| {
                self.collective_members(name)
                    .is_some_and(|members| members.iter().any(|m| m == member))
            })
            .cloned()
            .collect();
        ret.sort();
        ret.dedup();
        ret
    }

    /// Every promise a working agent makes, conditions and all, in
    /// declaration order without repeats. None when no such working agent.
    pub fn get_agent_promises(&self, agent_name: &str) -> Option<Vec<Behavior>> {
        let agents = self.working_agents.get(agent_name)?;
        let mut ret: Vec<Behavior> = vec![];
        for agent in agents {
            for behavior in agent.provides() {
                if !ret.contains(behavior) {
                    ret.push(behavior.clone());
                }
            }
        }
        Some(ret)
    }

    /// `name` as `agent` declares it — a promise, a condition or a want — with
    /// every value a substitution filled in wrapped in `<>`: `<A1> reads R1`
    /// for a want written `{{!self}} reads R1`.
    ///
    /// None when the agent wrote the name out in full, or does not declare it.
    /// For display only; the marked text is not a behavior name.
    pub fn marked_name(&self, agent: &str, name: &str) -> Option<String> {
        let variants = self.working_agents.get(agent)?;
        variants.iter().find_map(|variant| {
            variant
                .provides()
                .iter()
                .flat_map(|p| {
                    std::iter::once(p.get_name_pattern()).chain(p.get_condition_patterns())
                })
                .chain(variant.wants().iter().map(|w| w.get_name_pattern()))
                .find(|p| p.source() == name && p.has_bound())
                .map(|p| p.marked())
        })
    }

    /// Does any working agent make a promise that could keep the concrete
    /// `goal`, whether or not its conditions hold?
    pub fn has_provider(&self, goal: &str) -> bool {
        self.working_agents.values().any(|variants| {
            variants
                .iter()
                .any(|a| a.provides().iter().any(|b| b.match_goal(goal).is_some()))
        })
    }

    fn collect_members(&self, name: &str, visiting: &mut HashSet<String>, out: &mut Vec<String>) {
        if !visiting.insert(name.to_string()) {
            return;
        }
        let members: Vec<String> = self
            .available_superagents
            .iter()
            .filter(|sa| sa.get_name() == name)
            .flat_map(|sa| sa.get_agent_names())
            .collect();
        for member in members {
            let is_collective = self
                .available_superagents
                .iter()
                .any(|sa| sa.get_name() == &member);
            if is_collective {
                self.collect_members(&member, visiting, out);
            } else if !out.contains(&member) {
                out.push(member);
            }
        }
    }

    /// The comment on a working agent, if it has one. An instance carries
    /// its base's comment; a collective's members keep their own.
    pub fn get_agent_comment(&self, agent_name: &str) -> Option<String> {
        let agents = self.working_agents.get(agent_name)?;
        agents
            .iter()
            .map(|a| a.get_comment())
            .find(|c| !c.is_empty())
            .map(str::to_string)
    }

    /// The comment on one of an agent's promises, if the promise is written
    /// with one.
    pub fn get_behavior_comment(&self, agent_name: &str, behavior_name: &str) -> Option<String> {
        let agents = self.working_agents.get(agent_name)?;
        agents
            .iter()
            .flat_map(|a| a.provides().iter())
            .filter(|b| b.get_name() == behavior_name)
            .map(|b| b.get_comment())
            .find(|c| !c.is_empty())
            .map(str::to_string)
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

    /// [`Tracker::resolve`] for a want as `agent` declares it, so that the
    /// answer also knows which parts of the want the agent's own
    /// `{{!self}}` or Instance binding filled in
    /// ([`Resolution::marked_name`]). The offers are the same either way.
    pub fn resolve_want(&self, agent: &str, want: &str) -> Resolution {
        let resolved = self.resolve(want);
        match self.declared_want(agent, want) {
            Some(pattern) => as_reached_by(resolved, pattern),
            None => resolved,
        }
    }

    /// `want` as `agent` wrote it, when the agent wants it and a
    /// substitution filled part of it in.
    fn declared_want(&self, agent: &str, want: &str) -> Option<&Pattern> {
        self.working_agents.get(agent)?.iter().find_map(|variant| {
            variant
                .wants()
                .iter()
                .map(|w| w.get_name_pattern())
                .find(|p| p.source() == want && p.has_bound())
        })
    }

    /// `want` as `agent` wants it, with what the agent's own `{{!self}}` or
    /// Instance binding filled in wrapped in `<>`: `<A1> reads R1`. The same
    /// text as [`Tracker::resolve_want`]'s [`Resolution::marked_name`],
    /// without resolving anything.
    ///
    /// Only the want counts: a promise or condition of the same agent that
    /// reaches the same text through a variable adds nothing. The plain name
    /// when the agent wrote the want out, or does not want it. Display only;
    /// never a behavior name.
    pub fn marked_want(&self, agent: &str, want: &str) -> String {
        match self.declared_want(agent, want) {
            Some(pattern) => pattern.marked(),
            None => want.to_string(),
        }
    }

    /// Every circular dependency the contract holds, as the loop of goals that
    /// comes back around to itself.
    ///
    /// [`Tracker::resolve`] keeps nothing from a promise that depends on
    /// itself: the guard returns an empty resolution, which reads exactly like
    /// nobody having promised the goal at all. That is the right answer — a
    /// circular promise keeps nothing — but it is not the whole story, and this
    /// is what tells the two apart for anything that wants to say so.
    ///
    /// Each loop is listed once, rotated to start at its first name in sort
    /// order so that the same loop reached from two different goals is
    /// recognized as one. The loop closes back on its first entry: `["b1",
    /// "b2"]` is b1 depending on b2 depending on b1.
    pub fn dependency_cycles(&self) -> Vec<Vec<String>> {
        let mut roots: Vec<String> = self.get_working_behaviors().into_iter().collect();
        roots.sort();

        let mut found: BTreeSet<Vec<String>> = BTreeSet::new();
        // Goals whose dependencies have been walked all the way down without
        // being cut short, and so need not be walked again from another root.
        let mut clean: HashSet<String> = HashSet::new();
        for root in &roots {
            let mut in_progress: Vec<String> = vec![];
            self.walk_for_cycles(root, &mut in_progress, &mut clean, &mut found);
        }
        found.into_iter().collect()
    }

    /// The same descent [`Tracker::resolve_goal`] makes, keeping the loops
    /// rather than the offers.
    ///
    /// Returns whether the goal's dependencies were walked in full. A goal that
    /// was cut short — by the loop it sits on, or by the depth cap — is not
    /// recorded as clean, because what it reached depends on the path it was
    /// reached by.
    fn walk_for_cycles(
        &self,
        goal: &str,
        in_progress: &mut Vec<String>,
        clean: &mut HashSet<String>,
        found: &mut BTreeSet<Vec<String>>,
    ) -> bool {
        if let Some(at) = in_progress.iter().position(|g| g == goal) {
            found.insert(rotate_to_first(&in_progress[at..]));
            return false;
        }
        if in_progress.len() >= MAX_RESOLVE_DEPTH {
            return false;
        }
        if clean.contains(goal) {
            return true;
        }

        in_progress.push(goal.to_string());
        let mut complete = true;

        let mut agent_names: Vec<&String> = self.working_agents.keys().collect();
        agent_names.sort();
        for agent_name in agent_names {
            let Some(variants) = self.working_agents.get(agent_name) else {
                continue;
            };
            for variant_agent in variants {
                for (behavior, bindings) in variant_agent.get_matching_provides(goal) {
                    let conditions: Vec<Pattern> = behavior
                        .get_condition_patterns()
                        .iter()
                        .map(|c| c.substitute(&bindings))
                        .collect();
                    let readings = self.readings(&behavior, &conditions);
                    for condition in readings.groups.into_iter().flat_map(|g| g.readings).flatten() {
                        {
                            let sub_goal = match condition {
                                ConditionGoal::Goal(sub_goal) => sub_goal.source().clone(),
                                ConditionGoal::Gathered(pattern) => {
                                    complete &=
                                        self.walk_gathered(&pattern, in_progress, clean, found);
                                    continue;
                                }
                                ConditionGoal::Unbound(_) => continue,
                            };
                            complete &= self.walk_for_cycles(&sub_goal, in_progress, clean, found);
                        }
                    }
                }
            }
        }

        in_progress.pop();
        if complete {
            clean.insert(goal.to_string());
        }
        complete
    }

    /// Resolve one concrete goal.
    ///
    /// The answer is the same however the goal was reached, which is what
    /// lets `settled` hold it under the goal's text. What does depend on the
    /// way here — which parts of the goal's own name were substituted — is
    /// added afterwards by [`as_reached_by`], never stored.
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
        // Some promise's answer is uncertain because of the cap.
        let mut promise_cut_short = false;

        let mut agent_names: Vec<&String> = self.working_agents.keys().collect();
        agent_names.sort();
        for agent_name in agent_names {
            let Some(variants) = self.working_agents.get(agent_name) else {
                continue;
            };
            for variant_agent in variants {
                for (behavior, bindings) in variant_agent.get_matching_provides(goal) {
                    // The parts of the goal this promise names through a
                    // variable, for display: what its agent's own name or
                    // an Instance binding filled in, and what a
                    // parameterized name took from the goal.
                    let bound = behavior
                        .get_name_pattern()
                        .substitute(&bindings)
                        .bound()
                        .to_vec();
                    // if unconditional, add this as a satisfied Offer
                    if behavior.is_unconditional() {
                        r = r.add_satisfying_offer(Offer::new(agent_name).with_bound(bound));
                        continue;
                    }
                    // resolve conditions, with whatever the goal bound
                    // substituted into them — once for every reading, where
                    // a value one condition finds is shared with the others
                    let conditions: Vec<Pattern> = behavior
                        .get_condition_patterns()
                        .iter()
                        .map(|c| c.substitute(&bindings))
                        .collect();
                    let readings = self.readings(&behavior, &conditions);
                    let (combinations, stands_alone, cut_short) =
                        self.resolve_readings(readings, conditions.len(), in_progress, settled);
                    self_contained &= stands_alone;
                    promise_cut_short |= cut_short;
                    for resolved_conditions in combinations {
                        // if all conditions are satisfied, add this as a satisfied Offer
                        if resolved_conditions.iter().all(|x| x.is_satisfied()) {
                            r = r.add_satisfying_offer(
                                Offer::new_conditional(agent_name, resolved_conditions)
                                    .with_bound(bound.clone()),
                            );
                        // otherwise, add this as an unsatisfied Offer
                        } else {
                            r = r.add_unsatisfying_offer(
                                Offer::new_conditional(agent_name, resolved_conditions)
                                    .with_bound(bound.clone()),
                            );
                        }
                    }
                }
            }
        }

        // Kept by some offer, the answer is certain whatever the cap left.
        if promise_cut_short && !r.is_satisfied() {
            r = r.with_cut_short();
        }
        in_progress.pop();
        if self_contained {
            settled.insert(goal.to_string(), r.clone());
        }
        (r, self_contained)
    }
}

/// How many readings one promise's conditions may be given: the cap on the
/// values tried for one group of conditions tied together by shared
/// variables, and on the offers one promise makes for one goal. Each
/// condition that shares a value multiplies the readings by its matches, so
/// a contract can ask for far more than is worth resolving. When the cap
/// drops readings the goal's resolution says so
/// ([`Resolution::is_cut_short`]).
pub const MAX_CONDITION_READINGS: usize = 256;

/// How many partial readings (some shared variables still without a value)
/// one group may explore on the way to its complete ones.
const MAX_PARTIAL_READINGS: usize = MAX_CONDITION_READINGS * 16;

/// A promise's conditions as resolution reads them: split into groups that
/// no shared declared variable ties together, each with its own readings.
struct Readings {
    groups: Vec<ReadingGroup>,
}

/// Conditions tied together by shared declared variables, and the ways of
/// reading them: one list of goals per reading, in the order of
/// `conditions`.
struct ReadingGroup {
    /// The positions of the group's conditions among the promise's.
    conditions: Vec<usize>,
    readings: Vec<Vec<ConditionGoal>>,
    /// The cap left some of the group's readings unresolved.
    cut: bool,
    /// What the group reads, whatever its variables are called and wherever
    /// its conditions were written: its conditions' text with open
    /// variables written alike, sorted. Groups are put together in this
    /// order.
    order: Vec<String>,
}

/// How one condition is to be resolved under one reading of its promise.
enum ConditionGoal {
    /// Resolve this goal as it stands.
    Goal(Pattern),
    /// Still holding a variable its promise declares (`forSome`), shared with
    /// no other condition: one row gathering every match as alternatives.
    Gathered(Pattern),
    /// Still holding a variable nothing supplied a value for.
    Unbound(Pattern),
}

/// A goal's resolution, as the condition or want `reached` came to it: the
/// parts of the name a substitution filled in, and under a gathered row
/// where those and its variables fall in each behavior matched. Display
/// only; the offers are untouched.
fn as_reached_by(mut resolved: Resolution, reached: &Pattern) -> Resolution {
    if !reached.is_ground() {
        for offer in resolved.offers_mut() {
            let matched = offer.get_behavior().and_then(|kept| reached.spans_on(kept));
            if let Some(matched) = matched {
                *offer = offer.clone().with_matched(matched);
            }
        }
    }
    resolved.with_bound(reached.bound().to_vec())
}

impl Tracker {
    /// A promise's conditions, resolved together: one list per reading, in
    /// the order the conditions were given. There is more than one reading
    /// only where a condition finds a value its siblings share, and then one
    /// for each value found.
    ///
    /// `promise` is taken as it stands, conditions, declared variables and
    /// what was filled in alike, so that its conditions are read exactly as
    /// resolution reads them.
    pub fn resolve_conditions(&self, promise: &Behavior) -> Vec<Vec<Resolution>> {
        let patterns = promise.get_condition_patterns();
        let mut settled: HashMap<String, Resolution> = HashMap::new();
        let readings = self.readings(promise, patterns);
        self.resolve_readings(readings, patterns.len(), &mut vec![], &mut settled)
            .0
    }

    /// Resolve every group's readings and put them together into the
    /// promise's offers: one list of resolved conditions per offer, in the
    /// promise's condition order. Groups are independent, so an offer is one
    /// reading of each, and the promise is kept exactly when every group has
    /// a reading that holds. Readings that hold come first in each group.
    ///
    /// With more combinations than the cap on offers
    /// (`MAX_CONDITION_READINGS`), the ones shown are chosen so that every
    /// resolved reading of every group is in at least one: offer `i` takes
    /// reading `i mod n` of each group of `n` (the first of these is a kept
    /// combination when there is one), then the remaining combinations of
    /// readings that hold, then the rest, until the cap. Groups are taken in
    /// the order of what they read, so none of this depends on condition
    /// order or variable names.
    ///
    /// Also returns whether the answers stand on their own (see
    /// [`Tracker::resolve_goal`]), and whether the answer is cut short: not
    /// kept, and that only because the cap left a group unresolved — some
    /// cut group has no reading that holds, and every group with no reading
    /// that holds was cut.
    fn resolve_readings(
        &self,
        readings: Readings,
        count: usize,
        in_progress: &mut Vec<String>,
        settled: &mut HashMap<String, Resolution>,
    ) -> (Vec<Vec<Resolution>>, bool, bool) {
        let mut self_contained = true;
        // Each group's resolved readings, those that hold first; how many
        // hold; and whether it was cut.
        let mut groups: Vec<(Vec<usize>, Vec<Vec<Resolution>>, usize, bool)> = vec![];
        for group in readings.groups {
            let mut held = vec![];
            let mut not_held = vec![];
            for reading in group.readings {
                let (resolved, stands_alone) = self.resolve_reading(reading, in_progress, settled);
                self_contained &= stands_alone;
                if resolved.iter().all(|c| c.is_satisfied()) {
                    held.push(resolved);
                } else {
                    not_held.push(resolved);
                }
            }
            let holding = held.len();
            held.extend(not_held);
            groups.push((group.conditions, held, holding, group.cut));
        }
        let failing = |g: &&(Vec<usize>, Vec<Vec<Resolution>>, usize, bool)| g.2 == 0;
        let cut_short = groups.iter().filter(failing).any(|g| g.3)
            && groups.iter().filter(failing).all(|g| g.3);
        // One group over all the conditions in order: its readings are the
        // offers as they stand, nothing to put together or copy.
        if groups.len() == 1 && groups[0].0.iter().enumerate().all(|(i, p)| i == *p) {
            let (_, readings, _, _) = groups.pop().unwrap();
            return (readings, self_contained, cut_short);
        }
        let assemble = |choice: &[usize]| -> Vec<Resolution> {
            let mut out: Vec<Option<Resolution>> = vec![None; count];
            for ((positions, resolved, _, _), at) in groups.iter().zip(choice) {
                for (position, condition) in positions.iter().zip(&resolved[*at]) {
                    out[*position] = Some(condition.clone());
                }
            }
            out.into_iter().flatten().collect()
        };
        let holding: Vec<usize> = groups.iter().map(|g| g.2).collect();
        let all: Vec<usize> = groups.iter().map(|g| g.1.len()).collect();
        let mut chosen: Vec<Vec<usize>> = vec![];
        let mut taken: HashSet<Vec<usize>> = HashSet::new();
        let mut take = |choice: Vec<usize>, chosen: &mut Vec<Vec<usize>>| -> bool {
            if chosen.len() >= MAX_CONDITION_READINGS {
                return false;
            }
            if taken.insert(choice.clone()) {
                chosen.push(choice);
            }
            true
        };
        // Every reading of every group, at least once.
        let widest = all.iter().copied().max().unwrap_or(0);
        if all.iter().all(|n| *n > 0) {
            for i in 0..widest {
                take(all.iter().map(|n| i % n).collect(), &mut chosen);
            }
        }
        // Then the other kept combinations, then the rest.
        for_each_choice(&holding, |choice| take(choice.to_vec(), &mut chosen));
        for_each_choice(&all, |choice| take(choice.to_vec(), &mut chosen));
        let combinations = chosen.iter().map(|choice| assemble(choice)).collect();
        (combinations, self_contained, cut_short)
    }

    /// Resolve the conditions of one reading, and say whether the answers
    /// stand on their own (see [`Tracker::resolve_goal`]).
    fn resolve_reading(
        &self,
        reading: Vec<ConditionGoal>,
        in_progress: &mut Vec<String>,
        settled: &mut HashMap<String, Resolution>,
    ) -> (Vec<Resolution>, bool) {
        let mut self_contained = true;
        let mut resolved_conditions = Vec::new();
        for condition in reading {
            match condition {
                ConditionGoal::Goal(sub_goal) => {
                    let (resolved, stands_alone) =
                        self.resolve_goal(sub_goal.source(), in_progress, settled);
                    self_contained &= stands_alone;
                    resolved_conditions.push(as_reached_by(resolved, &sub_goal));
                }
                // The one match this reading stands on, kept under the
                // condition as written.
                ConditionGoal::Gathered(pattern) => {
                    let (resolved, stands_alone) =
                        self.resolve_gathered(&pattern, in_progress, settled);
                    self_contained &= stands_alone;
                    resolved_conditions.push(as_reached_by(resolved, &pattern));
                }
                ConditionGoal::Unbound(source) => {
                    resolved_conditions.push(
                        Resolution::new(source.source()).with_bound(source.bound().to_vec()),
                    );
                }
            }
        }
        (resolved_conditions, self_contained)
    }
}

/// What the candidate search proposes for the variables of one goal
/// ([`Tracker::propose`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Proposals {
    /// Values for the goal's variables, each the set one answering promise
    /// names together. A set may leave some variables out: the promise held
    /// for any value of those, or something further down named only some.
    /// In the order found (agents by name, then declaration order), each once.
    pub values: Vec<Bindings>,
    /// Promises that answered a variable while holding for any value of it,
    /// each once.
    pub any: Vec<HoldsForAny>,
}

/// A promise that answers one of a goal's variables while none of its own
/// conditions says anything about that value: it holds for any value, so it
/// proposes nothing, and accepts whatever something else proposes.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct HoldsForAny {
    /// The goal's variable.
    pub variable: String,
    /// The agent making the promise.
    pub agent: String,
    /// The promise, as written.
    pub promise: String,
}

/// A declared variable that can be anything ([`Tracker::unnamable_variables`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnnamableVariable {
    /// The agent whose promise declares the variable.
    pub agent: String,
    /// That promise, as written.
    pub promise: String,
    pub variable: String,
    /// The promises that answer it while holding for any value, in the
    /// order found; never empty.
    pub holders: Vec<HoldsForAny>,
}

impl std::fmt::Display for UnnamableVariable {
    /// "`kc` can be anything: B's `{{appname}} deploys to {{kc}}` holds for
    /// any value and nothing names one".
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let holders: Vec<String> = self
            .holders
            .iter()
            .map(|h| format!("{}'s `{}`", h.agent, h.promise))
            .collect();
        write!(
            f,
            "`{}` can be anything: {} {} for any value and nothing names one",
            self.variable,
            holders.join(" and "),
            if holders.len() == 1 { "holds" } else { "hold" }
        )
    }
}

impl Proposals {
    /// Does any proposal name a value for `variable`?
    pub fn names(&self, variable: &str) -> bool {
        self.values.iter().any(|v| v.contains_key(variable))
    }

    /// The promises that held for any value of `variable`.
    pub fn holding_for_any(&self, variable: &str) -> Vec<&HoldsForAny> {
        self.any.iter().filter(|a| a.variable == variable).collect()
    }

    fn renamed(&self, renaming: &BTreeMap<String, String>) -> Proposals {
        let rename = |v: &String| renaming.get(v).cloned().unwrap_or_else(|| v.clone());
        Proposals {
            values: self
                .values
                .iter()
                .map(|b| b.iter().map(|(k, v)| (rename(k), v.clone())).collect())
                .collect(),
            any: self
                .any
                .iter()
                .map(|a| HoldsForAny {
                    variable: rename(&a.variable),
                    ..a.clone()
                })
                .collect(),
        }
    }

    fn push_value(&mut self, value: Bindings) {
        // Naming nothing proposes nothing.
        if !value.is_empty() && !self.values.contains(&value) {
            self.values.push(value);
        }
    }

    fn push_any(&mut self, any: HoldsForAny) {
        if !self.any.contains(&any) {
            self.any.push(any);
        }
    }
}

/// The settled-cache key for a gathered row, kept apart from the concrete
/// goals the cache otherwise holds. What the row could be answered by
/// depends on which of its parts were filled in (the wording rule reads
/// them), so those are part of the key.
fn gathered_key(pattern: &Pattern) -> String {
    format!("\u{0}{}", pattern.key())
}

impl Tracker {
    /// The ways a promise's conditions can be read: by groups of conditions
    /// that share declared variables (`declared_readings`).
    fn readings(&self, promise: &Behavior, conditions: &[Pattern]) -> Readings {
        if promise.get_for_some().is_empty() {
            // Nothing declared: one reading, each condition as it stands, as
            // before declared variables existed.
            let reading = conditions
                .iter()
                .map(|c| {
                    if c.is_ground() {
                        ConditionGoal::Goal(c.clone())
                    } else {
                        ConditionGoal::Unbound(c.clone())
                    }
                })
                .collect();
            return Readings {
                groups: vec![ReadingGroup {
                    conditions: (0..conditions.len()).collect(),
                    readings: vec![reading],
                    cut: false,
                    order: vec![],
                }],
            };
        }
        self.declared_readings(conditions, promise.get_for_some())
    }

    /// The readings of conditions holding variables their promise declares.
    ///
    /// A declared variable two or more conditions use is shared, and ties
    /// those conditions into one group; conditions no shared variable ties
    /// together are groups of their own, read independently, so values
    /// they find do not multiply against each other. In a group, every set
    /// of candidate values the search proposes for its shared variables is
    /// one reading, with the values filled in throughout. A condition still
    /// holding a declared variable after that is gathered: one row, any
    /// match of which satisfies it. When no candidate is found for a group
    /// there is one reading, its conditions left as written, so the promise
    /// shows as an offer that cannot be kept.
    fn declared_readings(&self, conditions: &[Pattern], declared: &[String]) -> Readings {
        let uses = |c: &Pattern, v: &str| c.vars().contains(v);
        let shared: Vec<String> = declared
            .iter()
            .filter(|v| conditions.iter().filter(|c| uses(c, v)).count() >= 2)
            .cloned()
            .collect();
        let read = |c: Pattern| {
            if c.is_ground() {
                ConditionGoal::Goal(c)
            } else if c.vars().iter().all(|v| declared.iter().any(|d| d == v))
                && !shared.iter().any(|v| uses(&c, v))
            {
                ConditionGoal::Gathered(c)
            } else {
                ConditionGoal::Unbound(c)
            }
        };
        // Conditions tied together by a shared variable, by first position.
        let mut group_of: Vec<usize> = (0..conditions.len()).collect();
        fn root(group_of: &mut Vec<usize>, i: usize) -> usize {
            let mut i = i;
            while group_of[i] != i {
                group_of[i] = group_of[group_of[i]];
                i = group_of[i];
            }
            i
        }
        for variable in &shared {
            let tied: Vec<usize> = (0..conditions.len())
                .filter(|i| uses(&conditions[*i], variable))
                .collect();
            for pair in tied.windows(2) {
                let (a, b) = (root(&mut group_of, pair[0]), root(&mut group_of, pair[1]));
                group_of[a.max(b)] = a.min(b);
            }
        }
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..conditions.len() {
            let r = root(&mut group_of, i);
            members.entry(r).or_default().push(i);
        }

        let mut groups = vec![];
        for (_, positions) in members {
            let mine: Vec<Pattern> = positions.iter().map(|i| conditions[*i].clone()).collect();
            let their_shared: Vec<String> = shared
                .iter()
                .filter(|v| mine.iter().any(|c| uses(c, v)))
                .cloned()
                .collect();
            let (values, cut) = self.candidate_values(&mine, &their_shared);
            let mut order: Vec<String> = mine.iter().map(unnamed_text).collect();
            order.sort();
            let readings = if values.is_empty() {
                vec![mine.iter().cloned().map(read).collect()]
            } else {
                values
                    .iter()
                    .map(|value| mine.iter().map(|c| read(c.substitute(value))).collect())
                    .collect()
            };
            groups.push(ReadingGroup {
                conditions: positions,
                readings,
                cut,
                order,
            });
        }
        groups.sort_by(|a, b| a.order.cmp(&b.order));
        Readings { groups }
    }

    /// Every complete set of values for `shared` the conditions propose,
    /// sorted, and whether the cap cut any off. Each condition mentioning a
    /// variable still missing proposes, with what the reading has so far
    /// filled in, and the proposals are pooled; a reading that cannot be
    /// completed is dropped. With nothing shared, the one empty reading.
    ///
    /// Readings are explored nearest to complete first, then in the order
    /// of their values, and a reading's proposals are taken sorted, so
    /// which ones the cap keeps does not depend on the order the conditions
    /// were written in. Only complete readings count towards
    /// `MAX_CONDITION_READINGS`; partial ones have their own, larger cap.
    fn candidate_values(&self, conditions: &[Pattern], shared: &[String]) -> (Vec<Bindings>, bool) {
        // A reading's place in line: how many shared variables it still
        // misses, then the conditions as it fills them in, sorted, with any
        // variable still open written the same whatever it is called. None
        // of that depends on what a variable is called or where a condition
        // was written, so neither can change which readings the cap keeps.
        // The values themselves only break ties between readings that read
        // the same, and so make the same offer.
        type Place = (usize, Vec<String>, Bindings);
        let place = |b: Bindings| -> Place {
            let missing = shared.iter().filter(|v| !b.contains_key(*v)).count();
            let mut texts: Vec<String> = conditions
                .iter()
                .map(|c| unnamed_text(&c.substitute(&b)))
                .collect();
            texts.sort();
            (missing, texts, b)
        };
        let mut done: BTreeSet<(Vec<String>, Bindings)> = BTreeSet::new();
        let mut seen: BTreeSet<Bindings> = BTreeSet::new();
        let mut todo: BTreeSet<Place> = BTreeSet::new();
        todo.insert(place(Bindings::new()));
        let mut memo: HashMap<String, Proposals> = HashMap::new();
        let mut cut = false;
        while let Some((missing_count, texts, reading)) = todo.pop_first() {
            if missing_count == 0 {
                if done.len() < MAX_CONDITION_READINGS {
                    done.insert((texts, reading));
                } else {
                    cut = true;
                }
                continue;
            }
            let missing: Vec<&String> = shared
                .iter()
                .filter(|v| !reading.contains_key(*v))
                .collect();
            let mut next_readings: BTreeSet<Place> = BTreeSet::new();
            for condition in conditions {
                let vars = condition.vars();
                if !missing.iter().any(|v| vars.contains(v.as_str())) {
                    continue;
                }
                let goal = condition.substitute(&reading);
                let (found, _) = self.search(&goal, &mut vec![], &mut memo);
                for proposal in found.values {
                    let mut next = reading.clone();
                    let before = next.len();
                    next.extend(proposal.into_iter().filter(|(k, _)| missing.contains(&k)));
                    if next.len() > before && !seen.contains(&next) {
                        next_readings.insert(place(next));
                    }
                }
            }
            for next in next_readings {
                if seen.len() >= MAX_PARTIAL_READINGS {
                    cut = true;
                    break;
                }
                seen.insert(next.2.clone());
                todo.insert(next);
            }
        }
        (done.into_iter().map(|(_, b)| b).collect(), cut)
    }

    /// The concrete behaviors a gathered row could be answered by: every
    /// complete filling-in of its variables the search proposes, in the order
    /// found, each once.
    fn gathered_names(&self, pattern: &Pattern) -> Vec<String> {
        let mut names: Vec<String> = vec![];
        let mut seen: BTreeSet<Bindings> = BTreeSet::new();
        let mut todo: std::collections::VecDeque<Bindings> = [Bindings::new()].into();
        let mut memo: HashMap<String, Proposals> = HashMap::new();
        while let Some(reading) = todo.pop_front() {
            let goal = pattern.substitute(&reading);
            if goal.is_ground() {
                if !names.contains(goal.source()) {
                    names.push(goal.source().clone());
                }
                continue;
            }
            let vars = goal.vars();
            let (found, _) = self.search(&goal, &mut vec![], &mut memo);
            for proposal in found.values {
                let mut next = reading.clone();
                let before = next.len();
                next.extend(proposal.into_iter().filter(|(k, _)| vars.contains(k.as_str())));
                if next.len() > before && seen.insert(next.clone()) {
                    todo.push_back(next);
                }
            }
        }
        names
    }

    /// A gathered row: one resolution under the condition's pattern text,
    /// with the offers of every behavior it could be answered by, each
    /// naming the behavior it keeps. Cached by the pattern text, like a
    /// concrete goal; what depends on how it was reached is put on by
    /// [`as_reached_by`].
    fn resolve_gathered(
        &self,
        pattern: &Pattern,
        in_progress: &mut Vec<String>,
        settled: &mut HashMap<String, Resolution>,
    ) -> (Resolution, bool) {
        let text = pattern.source();
        if in_progress.iter().any(|g| g == text) || in_progress.len() >= MAX_RESOLVE_DEPTH {
            return (Resolution::new(text), false);
        }
        if let Some(known) = settled.get(&gathered_key(pattern)) {
            return (known.clone(), true);
        }
        in_progress.push(text.clone());
        let plain = Pattern::parse_lossy(text);
        let mut r = Resolution::new(text);
        let mut self_contained = true;
        let mut gathered_cut_short = false;
        for name in self.gathered_names(pattern) {
            let (resolved, stands_alone) = self.resolve_goal(&name, in_progress, settled);
            self_contained &= stands_alone;
            // The row stands for each behavior it gathers: if one of those
            // was cut short, so is the row, unless another keeps it.
            gathered_cut_short |= resolved.is_cut_short();
            let matched = plain.spans_on(&name).unwrap_or_default();
            for offer in resolved.get_satisfying_offers() {
                r = r.add_satisfying_offer(
                    offer.clone().answering(&name).with_matched(matched.clone()),
                );
            }
            for offer in resolved.get_unsatisfying_offers() {
                r = r.add_unsatisfying_offer(
                    offer.clone().answering(&name).with_matched(matched.clone()),
                );
            }
        }
        if gathered_cut_short && !r.is_satisfied() {
            r = r.with_cut_short();
        }
        in_progress.pop();
        if self_contained {
            settled.insert(gathered_key(pattern), r.clone());
        }
        (r, self_contained)
    }

    /// [`Tracker::walk_for_cycles`] through a gathered row: it depends on
    /// every behavior it could be answered by, and sits on the loop under its
    /// pattern text.
    fn walk_gathered(
        &self,
        pattern: &Pattern,
        in_progress: &mut Vec<String>,
        clean: &mut HashSet<String>,
        found: &mut BTreeSet<Vec<String>>,
    ) -> bool {
        let text = pattern.source();
        if let Some(at) = in_progress.iter().position(|g| g == text) {
            found.insert(rotate_to_first(&in_progress[at..]));
            return false;
        }
        if in_progress.len() >= MAX_RESOLVE_DEPTH {
            return false;
        }
        let key = gathered_key(pattern);
        if clean.contains(&key) {
            return true;
        }
        in_progress.push(text.clone());
        let mut complete = true;
        for name in self.gathered_names(pattern) {
            complete &= self.walk_for_cycles(&name, in_progress, clean, found);
        }
        in_progress.pop();
        if complete {
            clean.insert(key);
        }
        complete
    }

    /// What promises propose for the variables of `goal`, a name still
    /// holding some: the candidate search, two-sided
    /// ([`Pattern::unify`]). See [`Proposals`].
    ///
    /// Display and checks use this; resolution uses it to choose candidates
    /// and leaves deciding what holds to the concrete resolver.
    pub fn propose(&self, goal: &str) -> Proposals {
        self.propose_pattern(&Pattern::parse_lossy(goal))
    }

    /// [`Tracker::propose`] for a name with the parts already filled in
    /// marked ([`Pattern::bound`]), which the wording rule treats as values.
    pub fn propose_pattern(&self, goal: &Pattern) -> Proposals {
        self.search(goal, &mut vec![], &mut HashMap::new()).0
    }

    /// The candidate search. For each promise whose name matches `goal`
    /// two-sided: a variable that took the promise's text has that text as a
    /// value; one lined up with a promise variable is looked for in the
    /// promise's conditions that mention it, with what the match filled in,
    /// and if none does the promise holds for any value of it.
    ///
    /// Guarded like resolution: an in-progress stack of goals with their
    /// variables renamed ([`Pattern::canonical`]), so the same question under
    /// other names is recognized, and the depth cap. A goal cut short by
    /// either proposes nothing, and the second return value is false.
    /// Answers not cut short are kept in `memo`, by the renamed text.
    fn search(
        &self,
        goal: &Pattern,
        stack: &mut Vec<String>,
        memo: &mut HashMap<String, Proposals>,
    ) -> (Proposals, bool) {
        let (canonical, renaming) = goal.canonical();
        let back: BTreeMap<String, String> = renaming
            .into_iter()
            .map(|(written, renamed)| (renamed, written))
            .collect();
        // Text and spans: the wording rule reads what was filled in, so the
        // same text with other parts filled in is another question.
        let key = canonical.key();
        if stack.contains(&key) || stack.len() >= MAX_RESOLVE_DEPTH {
            return (Proposals::default(), false);
        }
        if let Some(known) = memo.get(&key) {
            return (known.renamed(&back), true);
        }
        stack.push(key.clone());
        let mut found = Proposals::default();
        let mut complete = true;

        let mut agent_names: Vec<&String> = self.working_agents.keys().collect();
        agent_names.sort();
        for agent_name in agent_names {
            for variant in &self.working_agents[agent_name] {
                for promise in variant.provides() {
                    let Some(unifier) = canonical.unify(promise.get_name_pattern()) else {
                        continue;
                    };
                    let taken = unifier.left_values();
                    let aligned = unifier.aligned();
                    if aligned.is_empty() {
                        found.push_value(taken);
                        continue;
                    }
                    let known = unifier.right_values();
                    let conditions: Vec<Pattern> = promise
                        .get_condition_patterns()
                        .iter()
                        .map(|c| c.substitute(&known))
                        .collect();
                    let mentions = |c: &Pattern, v: &str| c.vars().contains(v);
                    for (ours, theirs) in &aligned {
                        if !conditions.iter().any(|c| mentions(c, theirs)) {
                            found.push_any(HoldsForAny {
                                variable: ours.clone(),
                                agent: agent_name.clone(),
                                promise: promise.get_name().clone(),
                            });
                        }
                    }
                    let asking: Vec<&Pattern> = conditions
                        .iter()
                        .filter(|c| aligned.iter().any(|(_, theirs)| mentions(c, theirs)))
                        .collect();
                    if asking.is_empty() {
                        found.push_value(taken);
                        continue;
                    }
                    for condition in asking {
                        let (deeper, whole) = self.search(condition, stack, memo);
                        complete &= whole;
                        for value in deeper.values {
                            let mut proposal = taken.clone();
                            for (ours, theirs) in &aligned {
                                if let Some(v) = value.get(theirs) {
                                    proposal.insert(ours.clone(), v.clone());
                                }
                            }
                            found.push_value(proposal);
                        }
                        for any in deeper.any {
                            if let Some((ours, _)) =
                                aligned.iter().find(|(_, theirs)| *theirs == any.variable)
                            {
                                found.push_any(HoldsForAny {
                                    variable: ours.clone(),
                                    ..any
                                });
                            }
                        }
                    }
                }
            }
        }

        stack.pop();
        if complete {
            memo.insert(key, found.clone());
        }
        (found.renamed(&back), complete)
    }
}

/// `pattern`'s text with every variable written `{{}}`, whatever it is
/// called: for ordering readings without regard to variable names.
fn unnamed_text(pattern: &Pattern) -> String {
    pattern
        .segments()
        .iter()
        .map(|s| match s {
            components::Segment::Literal(l) => l.as_str(),
            components::Segment::Var(_) => "{{}}",
        })
        .collect()
}

/// Call `each` with every way of choosing one of `sizes[i]` things for each
/// `i`, the last position varying fastest, until it returns false. Nothing
/// when any size is zero; one empty choice when there are no positions.
fn for_each_choice(sizes: &[usize], mut each: impl FnMut(&[usize]) -> bool) {
    if sizes.iter().any(|n| *n == 0) {
        return;
    }
    let mut choice = vec![0; sizes.len()];
    loop {
        if !each(&choice) {
            return;
        }
        let mut at = sizes.len();
        loop {
            if at == 0 {
                return;
            }
            at -= 1;
            choice[at] += 1;
            if choice[at] < sizes[at] {
                break;
            }
            choice[at] = 0;
        }
    }
}

/// One loop rotated to begin at its first name in sort order, so the same loop
/// entered at two different points comes out as one answer.
fn rotate_to_first(loop_goals: &[String]) -> Vec<String> {
    let start = loop_goals
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(i, _)| i)
        .unwrap_or(0);
    loop_goals[start..]
        .iter()
        .chain(loop_goals[..start].iter())
        .cloned()
        .collect()
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
            fold_collectives: true,
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

    /// An agent may sit in two collectives. Folded, each collective carries
    /// it whole and stands for it; unfolded, it stands once on its own.
    #[test]
    fn test_agent_in_two_superagents() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("m").with_provides(vec![Behavior::build("b")]));
        t.add_agent(Agent::build("x").with_provides(vec![Behavior::build("bx")]));
        t.add_agent(Agent::build("c").with_wants(vec![Behavior::build("b")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("m")
                .with_agent("x"),
        );
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("m"));

        let mut working = t.get_working_agent_names();
        working.sort();
        assert_eq!(working, vec!["c", "sa1", "sa2"]);
        assert_eq!(
            t.resolve("b"),
            Resolution::new("b")
                .add_satisfying_offer(Offer::new("sa1"))
                .add_satisfying_offer(Offer::new("sa2"))
        );
        assert_eq!(t.folding_collectives("m"), vec!["sa1", "sa2"]);

        t.set_fold_collectives(false);
        let mut working = t.get_working_agent_names();
        working.sort();
        assert_eq!(working, vec!["c", "m", "x"]);
        assert_eq!(
            t.resolve("b"),
            Resolution::new("b").add_satisfying_offer(Offer::new("m"))
        );
    }

    /// A collective that lists an instance by name does not take it in:
    /// members are looked up among collectives and plain agents only. Folded,
    /// the collective carries nothing of the instance and the instance still
    /// stands on its own, so the UI does not offer to build one.
    #[test]
    fn test_instance_as_superagent_member_is_not_folded_in() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("base").with_provides(vec![Behavior::build("b")]));
        t.add_agent(Agent::build("c").with_wants(vec![Behavior::build("b")]));
        t.add_instance(Instance::new("i1", "Agent/base"));
        t.add_superagent(SuperAgent::new(String::from("sa")).with_agent("i1"));

        let mut working = t.get_working_agent_names();
        working.sort();
        assert_eq!(working, vec!["c", "i1", "sa"]);
        assert_eq!(t.get_agent_provides("sa"), Some(HashSet::new()));
        assert_eq!(
            t.resolve("b"),
            Resolution::new("b").add_satisfying_offer(Offer::new("i1"))
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
    fn test_membership_cycles_reports_two_collectives_containing_each_other() {
        let mut t = Tracker::new();
        t.add_superagent(SuperAgent::new(String::from("sa1")).with_agent("sa2"));
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("sa1"));

        assert_eq!(
            t.membership_cycles(),
            vec![vec!["sa1".to_string(), "sa2".to_string()]]
        );
        // and the reason it is worth saying: neither is left standing
        assert!(t.get_working_agent_names().is_empty());
    }

    #[test]
    fn test_membership_cycles_reports_a_collective_listing_itself() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("sa1"),
        );

        assert_eq!(t.membership_cycles(), vec![vec!["sa1".to_string()]]);
        // a1 is folded in, and sa1 makes a template of itself, so b1 is gone
        assert!(t.get_working_agent_names().is_empty());
        assert_eq!(t.resolve("b1"), Resolution::new("b1"));
    }

    #[test]
    fn test_membership_cycles_is_empty_for_plain_nesting() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_superagent(SuperAgent::new(String::from("sa1")).with_agent("a1"));
        t.add_superagent(
            SuperAgent::new(String::from("sa2"))
                .with_agent("sa1")
                .with_agent("a2"),
        );

        assert!(t.membership_cycles().is_empty());
        // Two collectives naming the same member is sharing, not a loop.
        t.add_superagent(SuperAgent::new(String::from("sa3")).with_agent("sa1"));
        assert!(t.membership_cycles().is_empty());
    }

    #[test]
    fn test_membership_cycles_reports_a_longer_ring_once() {
        let mut t = Tracker::new();
        t.add_superagent(SuperAgent::new(String::from("sa1")).with_agent("sa2"));
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("sa3"));
        t.add_superagent(SuperAgent::new(String::from("sa3")).with_agent("sa1"));

        // Reachable from all three names; reported as one loop.
        assert_eq!(
            t.membership_cycles(),
            vec![vec![
                "sa1".to_string(),
                "sa2".to_string(),
                "sa3".to_string()
            ]]
        );
    }

    #[test]
    fn add_items_matches_adding_one_at_a_time() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/kubernetes");
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
            .collect();
        paths.sort();
        let texts: Vec<String> = paths
            .into_iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        // Every file twice over, so duplicates are skipped the same way by both.
        let items = || {
            let mut items = Vec::new();
            for text in texts.iter().chain(texts.iter()) {
                for document in serde_yaml::Deserializer::from_str(text) {
                    items.push(<Item as serde::Deserialize>::deserialize(document).unwrap());
                }
            }
            items
        };

        for fold in [true, false] {
            let mut one_by_one = Tracker::new();
            one_by_one.set_fold_collectives(fold);
            for item in items() {
                one_by_one.add_item(item);
            }
            let mut batched = Tracker::new();
            batched.set_fold_collectives(fold);
            batched.add_items(items());

            assert!(!batched.is_empty());
            // Same inputs kept, and the same working agents built from them
            // (compared in name order: working_agents is a HashMap).
            let view = |t: &Tracker| {
                let mut working: Vec<String> = t
                    .working_agents
                    .iter()
                    .map(|(name, agents)| format!("{name}: {agents:?}"))
                    .collect();
                working.sort();
                (
                    format!("{:?}", t.available_agents),
                    format!("{:?}", t.available_superagents),
                    format!("{:?}", t.available_instances),
                    working,
                )
            };
            assert_eq!(view(&batched), view(&one_by_one));
            assert_eq!(
                promise_graph::promise_graph(&batched),
                promise_graph::promise_graph(&one_by_one)
            );
        }
    }

    #[test]
    fn test_membership_cycles_from_a_written_contract() {
        let mut t = Tracker::new();
        for document in serde_yaml::Deserializer::from_str(
            "kind: SuperAgent
name: sa1
agents:
  - sa2
---
kind: SuperAgent
name: sa2
agents:
  - sa1
",
        ) {
            t.add_item(<Item as serde::Deserialize>::deserialize(document).unwrap());
        }

        assert_eq!(
            t.membership_cycles(),
            vec![vec!["sa1".to_string(), "sa2".to_string()]]
        );
    }

    #[test]
    fn test_dependency_cycles_reports_a_two_agent_loop() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("b").with_provides(vec![
            Behavior::build("b2").with_conditions(vec![String::from("b1")]),
        ]));

        // The same loop is reachable from either goal; it is reported once.
        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["b1".to_string(), "b2".to_string()]]
        );
    }

    #[test]
    fn test_dependency_cycles_from_a_written_contract() {
        // The path the editor takes: YAML documents in, cycles out.
        let mut t = Tracker::new();
        for document in serde_yaml::Deserializer::from_str(
            "kind: Agent
name: w1
wants:
  - name: web
---
kind: Agent
name: app
provides:
  - name: web
    conditions: [db]
---
kind: Agent
name: db
provides:
  - name: db
    conditions: [web]
",
        ) {
            t.add_item(<Item as serde::Deserialize>::deserialize(document).unwrap());
        }

        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["db".to_string(), "web".to_string()]]
        );
        // and the resolution it explains: the want reads as unpromised
        assert!(!t.resolve("web").is_satisfied());
    }

    #[test]
    fn test_dependency_cycles_is_empty_without_one() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("b").with_provides(vec![Behavior::build("b2")]));

        assert!(t.dependency_cycles().is_empty());
        // An unanswered goal is not a loop either, however deep the chain.
        t.add_agent(Agent::build("c").with_provides(vec![
            Behavior::build("b3").with_conditions(vec![String::from("nobody-promises-this")]),
        ]));
        assert!(t.dependency_cycles().is_empty());
    }

    #[test]
    fn test_dependency_cycles_reports_each_loop_once() {
        let mut t = Tracker::new();
        // b1 -> b2 -> b1, and a separate b3 -> b4 -> b5 -> b3
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
            Behavior::build("b2").with_conditions(vec![String::from("b1")]),
            Behavior::build("b3").with_conditions(vec![String::from("b4")]),
            Behavior::build("b4").with_conditions(vec![String::from("b5")]),
            Behavior::build("b5").with_conditions(vec![String::from("b3")]),
        ]));

        assert_eq!(
            t.dependency_cycles(),
            vec![
                vec!["b1".to_string(), "b2".to_string()],
                vec!["b3".to_string(), "b4".to_string(), "b5".to_string()],
            ]
        );
    }

    #[test]
    fn test_dependency_cycles_reports_a_promise_that_depends_on_itself() {
        let mut t = Tracker::new();
        // b2 needing itself, reached through a promise that needs b2.
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("b").with_provides(vec![
            Behavior::build("b2").with_conditions(vec![String::from("b2")]),
        ]));

        assert_eq!(t.dependency_cycles(), vec![vec!["b2".to_string()]]);
    }

    #[test]
    fn test_dependency_cycles_survives_being_folded_into_one_collective() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("web")]));
        t.add_agent(Agent::build("app").with_provides(vec![
            Behavior::build("web").with_conditions(vec![String::from("db")]),
        ]));
        t.add_agent(Agent::build("dbsvc").with_provides(vec![
            Behavior::build("db").with_conditions(vec![String::from("web")]),
        ]));
        t.add_superagent(
            SuperAgent::new(String::from("stack"))
                .with_agent("app")
                .with_agent("dbsvc"),
        );

        // Folding the loop into one working agent is where it used to
        // disappear: reduction dropped the condition that came back around and
        // left the collective promising web outright. What the collective can
        // say is that each half needs itself.
        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["db".to_string()], vec!["web".to_string()]]
        );
        assert!(!t.resolve("web").is_satisfied());
    }

    #[test]
    fn test_dependency_cycles_follows_a_parameterized_loop() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("p/x")]));
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("p/{{v}}").with_conditions(vec![String::from("q/{{v}}")]),
        ]));
        t.add_agent(Agent::build("b").with_provides(vec![
            Behavior::build("q/{{v}}").with_conditions(vec![String::from("p/{{v}}")]),
        ]));

        // The loop only exists once a want grounds the family, so it is named
        // by the concrete goals the want reaches.
        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["p/x".to_string(), "q/x".to_string()]]
        );
    }

    #[test]
    fn test_dependency_cycles_survives_a_collective() {
        let mut t = Tracker::new();
        // Folded into one collective, reduce() would close the loop internally;
        // split across two, it reaches resolution.
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b2").with_conditions(vec![String::from("b1")]),
        ]));
        t.add_superagent(SuperAgent::new(String::from("sa1")).with_agent("a1"));
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("a2"));

        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["b1".to_string(), "b2".to_string()]]
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

    /// The motivating chain: A1 and D1 name themselves, and it resolves as
    /// if they had written their names out.
    #[test]
    fn test_self_is_the_agent_that_wrote_it() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("A1").with_wants(vec![Behavior::build("{{!self}} reads R1")]));
        t.add_agent(Agent::build("S1").with_provides(vec![
            Behavior::build("{{agent}} reads R1").with_conditions(vec![String::from(
                "{{!SELF}} configured to allow R1 read access for {{agent}}",
            )]),
        ]));
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build(
            "S1 configured to allow R1 read access for {{agent}}",
        )
        .with_conditions(vec![String::from("D1 shows usage of R1 by {{agent}}")])]));
        t.add_agent(
            Agent::build("D1")
                .with_provides(vec![Behavior::build("{{!self}} shows usage of R1 by A1")]),
        );

        assert!(t.non_ground_wants().is_empty());
        assert_eq!(
            t.get_agent_wants(String::from("A1")),
            HashSet::from([String::from("A1 reads R1")])
        );
        assert!(t.resolve("A1 reads R1").is_satisfied());
        assert_eq!(
            t.resolve("D1 shows usage of R1 by A1"),
            Resolution::new("D1 shows usage of R1 by A1").add_satisfying_offer(Offer::new("D1"))
        );

        // what was written as a variable can be shown as such
        assert_eq!(
            t.marked_name("A1", "A1 reads R1"),
            Some("<A1> reads R1".to_string())
        );
        assert_eq!(
            t.marked_name("D1", "D1 shows usage of R1 by A1"),
            Some("<D1> shows usage of R1 by A1".to_string())
        );
        // O1 wrote S1 out in full, and nobody else declares A1's want
        assert_eq!(
            t.marked_name("O1", "S1 configured to allow R1 read access for {{agent}}"),
            None
        );
        assert_eq!(t.marked_name("S1", "A1 reads R1"), None);
        assert_eq!(t.marked_name("nobody", "A1 reads R1"), None);
    }

    #[test]
    fn test_resolution_knows_what_was_substituted() {
        let t = crate::marks::tracker(crate::marks::IAM);
        let want = "Read Secret Contents appsecret by appname";

        // Resolved by text alone, nobody's declaration is in play.
        assert_eq!(t.resolve(want).marked_name(), want);
        // As appname wants it.
        let r = t.resolve_want("appname", want);
        assert_eq!(r.marked_name(), "Read Secret Contents appsecret by <appname>");
        assert_eq!(r, t.resolve(want));
        // An agent that does not want it adds nothing.
        assert_eq!(t.resolve_want("manager", want).marked_name(), want);

        // AWSSM's parameterized name took both values from the goal.
        let awssm = &r.get_satisfying_offers()[0];
        assert_eq!(
            r.marked_offer(awssm),
            "Read Secret Contents <appsecret> by <appname>"
        );
        assert_eq!(
            r.marked_name_with_offers(),
            "Read Secret Contents <appsecret> by <appname>"
        );

        // Its conditions, which share the declared `approle`: each with the
        // goal's value and the value found for `approle`.
        let conditions = awssm.get_resolved_conditions();
        assert_eq!(
            conditions[0].marked_name(),
            "<appname> has IAM role <approle>"
        );
        // The behavior kept: the reading's own text, and the provider's own
        // name.
        let role = &conditions[0].get_satisfying_offers()[0];
        assert_eq!(role.get_behavior(), None);
        assert_eq!(
            conditions[0].marked_offer(role),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(
            conditions[1].marked_name(),
            "IAM access granted for reading Secret <appsecret> to <approle>"
        );

        // One level down, infrateam's gathered row and manager's promise.
        let grant = &conditions[1].get_satisfying_offers()[0];
        let approval = &grant.get_resolved_conditions()[0];
        assert_eq!(
            approval.marked_name(),
            "Design approval access to Secret <appsecret> for {{application}}"
        );
        assert_eq!(
            approval.marked_offer(&approval.get_satisfying_offers()[0]),
            "Design approval access to Secret <appsecret> for <appname>"
        );

        // The whole of it at once. Equality cannot see any of this.
        assert_eq!(
            r.marked_outline(),
            vec![
                "Read Secret Contents appsecret by <appname>",
                "  AWSSM: Read Secret Contents <appsecret> by <appname>",
                "    <appname> has IAM role <approle>",
                "      appname: <appname> has IAM role <approle>",
                "    IAM access granted for reading Secret <appsecret> to <approle>",
                "      infrateam: IAM access granted for reading Secret <appsecret> to <approle>",
                "        Design approval access to Secret <appsecret> for {{application}}",
                "          manager: Design approval access to Secret <appsecret> for <appname>",
            ]
        );
        assert_eq!(
            t.resolve(want).marked_outline()[0],
            "Read Secret Contents appsecret by appname"
        );
    }

    #[test]
    fn test_marked_want_is_the_want_as_its_agent_wrote_it() {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("a1")
                .with_wants(vec![
                    Behavior::build("{{!self}} is up"),
                    Behavior::build("a2 is up"),
                ])
                // a promise and a condition reaching a want's text through a
                // variable say nothing about how the want was written
                .with_provides(vec![Behavior::build("{{who}} is up")
                    .with_conditions(vec![String::from("{{who}} is up")])]),
        );
        t.add_agent(Agent::build("a2").with_wants(vec![Behavior::build("{{!self}} is up")]));
        t.add_agent(
            Agent::build("\u{e9}1").with_wants(vec![Behavior::build("{{!self}} is up, \u{e9}1")]),
        );

        assert_eq!(t.marked_want("a1", "a1 is up"), "<a1> is up");
        assert_eq!(t.marked_want("a1", "a2 is up"), "a2 is up");
        assert_eq!(t.marked_want("a2", "a2 is up"), "<a2> is up");
        // not a want of that agent, or no such agent: the plain name
        assert_eq!(t.marked_want("a2", "a1 is up"), "a1 is up");
        assert_eq!(t.marked_want("nobody", "a1 is up"), "a1 is up");
        // only the occurrence that was substituted, on character boundaries
        assert_eq!(
            t.marked_want("\u{e9}1", "\u{e9}1 is up, \u{e9}1"),
            "<\u{e9}1> is up, \u{e9}1"
        );
        // the same text the resolution carries
        for (agent, want) in [("a1", "a1 is up"), ("a1", "a2 is up"), ("a2", "a2 is up")] {
            assert_eq!(
                t.marked_want(agent, want),
                t.resolve_want(agent, want).marked_name()
            );
        }
    }

    #[test]
    fn test_open_want_marks_what_it_matched() {
        // What an open want was is now a gathered row on a promise.
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("{{!self}} is safe")
            .with_for_some(vec!["who".into()])
            .with_conditions(vec!["{{who}} backs up {{!self}}".into()])]));
        t.add_agent(Agent::build("b1").with_provides(vec![Behavior::build("{{!self}} backs up a1")]));

        let r = t.resolve("a1 is safe");
        let row = &r.get_satisfying_offers()[0].get_resolved_conditions()[0];
        // As a1 wrote it: its own name, and what `{{who}}` matched.
        assert_eq!(row.marked_name(), "{{who}} backs up <a1>");
        assert_eq!(
            row.marked_offer(&row.get_satisfying_offers()[0]),
            "<b1> backs up <a1>"
        );
        assert_eq!(
            r.marked_outline(),
            vec![
                "a1 is safe",
                "  a1: <a1> is safe",
                "    {{who}} backs up <a1>",
                "      b1: <b1> backs up <a1>",
            ]
        );
        // By text alone, nobody's declaration is in play.
        assert_eq!(t.resolve("b1 backs up a1").marked_outline(), vec![
            "b1 backs up a1",
            "  b1: <b1> backs up a1",
        ]);
    }

    #[test]
    fn test_marks_follow_the_way_a_goal_was_reached_not_the_cache() {
        // `a2 is up` is reached twice under one want: written out by a1,
        // and as `{{!self}} is up` by a2. The settled cache holds one answer
        // for the text; each condition still shows what its own agent wrote.
        for order in [["mid1", "mid2"], ["mid2", "mid1"]] {
            let mut t = Tracker::new();
            t.add_agent(Agent::build("a0").with_provides(vec![Behavior::build("top")
                .with_conditions(order.iter().map(|c| c.to_string()).collect())]));
            t.add_agent(Agent::build("a1").with_provides(vec![
                Behavior::build("mid1").with_conditions(vec!["a2 is up".to_string()])
            ]));
            t.add_agent(Agent::build("a2").with_provides(vec![
                Behavior::build("mid2").with_conditions(vec!["{{!self}} is up".to_string()])
            ]));
            t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("a2 is up")]));

            let r = t.resolve("top");
            let under = |mid: &str| -> String {
                r.get_satisfying_offers()[0]
                    .get_resolved_conditions()
                    .iter()
                    .find(|c| c.get_behavior_name() == mid)
                    .unwrap()
                    .get_satisfying_offers()[0]
                    .get_resolved_conditions()[0]
                    .marked_name()
            };
            assert_eq!(under("mid1"), "a2 is up");
            assert_eq!(under("mid2"), "<a2> is up");
        }
    }

    #[test]
    fn test_self_is_the_instance_not_its_base() {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("reader")
                .with_wants(vec![Behavior::build("{{!self}} reads R1")])
                .with_provides(vec![Behavior::build("{{!self}}/{{env}} is up")]),
        );
        t.add_instance(Instance::new("r-prod", "reader").with_binding("env", "prod"));
        t.add_instance(
            Instance::new("r-dev", "reader")
                .with_binding("env", "dev")
                .with_wants(vec![Behavior::build("{{!self}} has a sandbox")]),
        );

        assert_eq!(t.get_working_agent_names(), vec!["r-dev", "r-prod"]);
        assert_eq!(
            t.get_agent_wants(String::from("r-prod")),
            HashSet::from([String::from("r-prod reads R1")])
        );
        assert_eq!(
            t.get_agent_wants(String::from("r-dev")),
            HashSet::from([
                String::from("r-dev reads R1"),
                String::from("r-dev has a sandbox")
            ])
        );
        assert!(t.non_ground_wants().is_empty());
        assert!(t.resolve("r-prod/prod is up").is_satisfied());
        assert!(!t.resolve("reader/prod is up").is_satisfied());
        // both kinds of substitution are marked
        assert_eq!(
            t.marked_name("r-prod", "r-prod/prod is up"),
            Some("<r-prod>/<prod> is up".to_string())
        );
    }

    #[test]
    fn test_self_is_the_member_folded_or_not() {
        let build = |fold: bool| {
            let mut t = Tracker::new();
            t.set_fold_collectives(fold);
            t.add_agent(Agent::build("db").with_wants(vec![Behavior::build("{{!self}} reads R1")]));
            t.add_agent(Agent::build("web").with_provides(vec![Behavior::build("{{!self}} serves")]));
            t.add_superagent(
                SuperAgent::new(String::from("site"))
                    .with_agent("db")
                    .with_agent("web"),
            );
            t
        };

        let folded = build(true);
        assert_eq!(folded.get_working_agent_names(), vec!["site"]);
        assert_eq!(
            folded.get_agent_wants(String::from("site")),
            HashSet::from([String::from("db reads R1")])
        );
        assert!(folded.resolve("web serves").is_satisfied());
        assert_eq!(
            folded.marked_name("site", "db reads R1"),
            Some("<db> reads R1".to_string())
        );

        let unfolded = build(false);
        assert_eq!(
            unfolded.get_agent_wants(String::from("db")),
            HashSet::from([String::from("db reads R1")])
        );
        assert!(unfolded.resolve("web serves").is_satisfied());
    }

    /// The documented wrinkle: a collective's members keep their own names, so
    /// two instances of it say the same thing unless a binding tells them
    /// apart.
    #[test]
    fn test_self_overlaps_across_instances_of_a_collective() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("db").with_wants(vec![
            Behavior::build("{{!self}} reads R1"),
            Behavior::build("{{!self}}/{{env}} reads R2"),
        ]));
        t.add_superagent(SuperAgent::new(String::from("site")).with_agent("db"));
        t.add_instance(Instance::new("prod", "SuperAgent/site").with_binding("env", "prod"));
        t.add_instance(Instance::new("staging", "SuperAgent/site").with_binding("env", "staging"));

        assert_eq!(
            t.get_agent_wants(String::from("prod")),
            HashSet::from([String::from("db reads R1"), String::from("db/prod reads R2")])
        );
        assert_eq!(
            t.get_agent_wants(String::from("staging")),
            HashSet::from([
                String::from("db reads R1"),
                String::from("db/staging reads R2")
            ])
        );
    }

    /// O1 does not care which document shows the usage, only that one does.
    fn open_chain(docs: &[(&str, bool)]) -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("A1").with_wants(vec![Behavior::build("{{!self}} reads R1")]));
        t.add_agent(Agent::build("S1").with_provides(vec![
            Behavior::build("{{agent}} reads R1").with_conditions(vec![String::from(
                "{{!self}} configured to allow R1 read access for {{agent}}",
            )]),
        ]));
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build(
            "S1 configured to allow R1 read access for {{agent}}",
        )
        .with_for_some(vec!["doc".into()])
        .with_conditions(vec![String::from("{{doc}} shows usage of R1 by {{agent}}")])]));
        for (doc, approved) in docs {
            let mut provides = vec![Behavior::build("{{!self}} shows usage of R1 by A1")
                .with_conditions(vec![String::from("{{!self}} is approved")])];
            if *approved {
                provides.push(Behavior::build("{{!self}} is approved"));
            }
            t.add_agent(Agent::build(doc).with_provides(provides));
        }
        t
    }

    /// The row O1's `{{doc}}` condition resolves to, under its first offer.
    fn usage_row(t: &Tracker) -> Resolution {
        let r = t.resolve("S1 configured to allow R1 read access for A1");
        let offer = r
            .get_satisfying_offers()
            .iter()
            .chain(r.get_unsatisfying_offers())
            .next()
            .unwrap()
            .clone();
        offer.get_resolved_conditions()[0].clone()
    }

    #[test]
    fn test_open_condition_is_met_by_whoever_provides_a_match() {
        let t = open_chain(&[("D1", true)]);
        assert!(t.resolve("A1 reads R1").is_satisfied());
        assert_eq!(
            usage_row(&t),
            Resolution::new("{{doc}} shows usage of R1 by A1").add_satisfying_offer(
                Offer::new_conditional(
                    "D1",
                    vec![Resolution::new("D1 is approved").add_satisfying_offer(Offer::new("D1"))]
                )
                .answering("D1 shows usage of R1 by A1")
            )
        );
        assert!(t.dependency_cycles().is_empty());
    }

    #[test]
    fn test_open_goal_gathers_every_match_and_any_one_keeps_it() {
        let t = open_chain(&[("D1", false), ("D2", true)]);
        let r = usage_row(&t);
        assert!(r.is_satisfied());
        assert_eq!(r.get_satisfying_offers().len(), 1);
        assert_eq!(
            r.get_satisfying_offers()[0].get_behavior(),
            Some("D2 shows usage of R1 by A1")
        );
        assert_eq!(r.get_unsatisfying_offers().len(), 1);
        assert_eq!(
            r.get_unsatisfying_offers()[0].get_behavior(),
            Some("D1 shows usage of R1 by A1")
        );
        assert!(t.resolve("A1 reads R1").is_satisfied());

        // neither approved: every match fails, and so does the chain
        let t = open_chain(&[("D1", false), ("D2", false)]);
        assert!(!t.resolve("A1 reads R1").is_satisfied());
    }

    #[test]
    fn test_open_goal_with_no_match_is_unsatisfied_under_its_own_text() {
        let t = open_chain(&[]);
        assert_eq!(usage_row(&t), Resolution::new("{{doc}} shows usage of R1 by A1"));
        assert!(!t.resolve("A1 reads R1").is_satisfied());
    }

    /// What `{{?x}}` in a want was: now literal text, which validation
    /// rejects. The search it stood for belongs on a promise, where it also
    /// meets a parameterized provider.
    #[test]
    fn test_open_want_is_literal_and_a_declared_variable_meets_a_parameterized_provider() {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("A1")
                .with_wants(vec![
                    Behavior::build("{{?svc}} grants {{!self}} read on R1"),
                    Behavior::build("{{!self}} can read R1"),
                ])
                .with_provides(vec![Behavior::build("{{!self}} can read R1")
                    .with_for_some(vec!["svc".into()])
                    .with_conditions(vec!["{{svc}} grants {{!self}} read on R1".into()])]),
        );
        // a pattern on the provider's side
        t.add_agent(
            Agent::build("S1")
                .with_provides(vec![Behavior::build("{{!self}} grants {{agent}} read on R1")]),
        );

        // the old open want is a name of its own, not a gap in the contract
        assert!(t.non_ground_wants().is_empty());
        assert!(t
            .get_agent_wants("A1".into())
            .contains("{{?svc}} grants {{!self}} read on R1"));
        assert!(!t.resolve("{{?svc}} grants {{!self}} read on R1").is_satisfied());
        // the declared variable finds S1 through its parameterized promise
        assert_eq!(
            t.resolve("A1 can read R1"),
            Resolution::new("A1 can read R1").add_satisfying_offer(Offer::new_conditional(
                "A1",
                vec![Resolution::new("{{svc}} grants A1 read on R1")
                    .add_satisfying_offer(Offer::new("S1").answering("S1 grants A1 read on R1"))]
            ))
        );
        // while a want with a plain variable is still a gap
        t.add_agent(Agent::build("W").with_wants(vec![Behavior::build("{{a}} and {{b}}")]));
        assert_eq!(
            t.non_ground_wants(),
            vec![("W".to_string(), "{{a}} and {{b}}".to_string())]
        );
    }

    /// O1 keeps `configured` when something it will support also does
    /// stuff: the two conditions have to be about the same thing.
    fn shared_value(does_stuff: &[&str]) -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_for_some(vec!["thing".into()])
            .with_conditions(vec![
                String::from("will support {{thing}}"),
                String::from("{{thing}} does stuff"),
            ])]));
        t.add_agent(Agent::build("S1").with_provides(vec![
            Behavior::build("will support T1"),
            Behavior::build("will support T2"),
        ]));
        for thing in does_stuff {
            t.add_agent(
                Agent::build(thing)
                    .with_provides(vec![Behavior::build(&format!("{} does stuff", thing))]),
            );
        }
        t
    }

    fn supports(thing: &str) -> Resolution {
        Resolution::new(&format!("will support {}", thing)).add_satisfying_offer(Offer::new("S1"))
    }

    #[test]
    fn test_shared_value_is_found_by_one_condition_and_used_by_another() {
        let t = shared_value(&["T2"]);
        assert_eq!(
            t.resolve("configured"),
            Resolution::new("configured")
                // T1 is supported, but does nothing
                .add_unsatisfying_offer(Offer::new_conditional(
                    "O1",
                    vec![supports("T1"), Resolution::new("T1 does stuff")]
                ))
                .add_satisfying_offer(Offer::new_conditional(
                    "O1",
                    vec![
                        supports("T2"),
                        Resolution::new("T2 does stuff").add_satisfying_offer(Offer::new("T2"))
                    ]
                ))
        );
    }

    #[test]
    fn test_shared_value_records_every_candidate_that_holds() {
        let t = shared_value(&["T1", "T2"]);
        let r = t.resolve("configured");
        assert_eq!(r.get_satisfying_offers().len(), 2);
        assert!(r.get_unsatisfying_offers().is_empty());
    }

    #[test]
    fn test_shared_value_does_not_mix_candidates() {
        // T3 does stuff, but nothing will support it. Every condition
        // proposes, so T3 is a candidate too; each reading is one value
        // throughout, and none holds.
        let t = shared_value(&["T3"]);
        let r = t.resolve("configured");
        assert!(!r.is_satisfied());
        let readings: Vec<Vec<String>> = r
            .get_unsatisfying_offers()
            .iter()
            .map(offer_conditions)
            .collect();
        assert_eq!(
            readings,
            vec![
                vec!["will support T1".to_string(), "T1 does stuff".to_string()],
                vec!["will support T2".to_string(), "T2 does stuff".to_string()],
                vec!["will support T3".to_string(), "T3 does stuff".to_string()],
            ]
        );
    }

    #[test]
    fn test_shared_value_nothing_found_leaves_the_rest_as_written() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_for_some(vec!["thing".into()])
            .with_conditions(vec![
                String::from("{{thing}} does stuff"),
                String::from("will support {{thing}}"),
            ])]));
        assert_eq!(
            t.resolve("configured"),
            Resolution::new("configured").add_unsatisfying_offer(Offer::new_conditional(
                "O1",
                vec![
                    Resolution::new("{{thing}} does stuff"),
                    Resolution::new("will support {{thing}}")
                ]
            ))
        );
        assert!(t.dependency_cycles().is_empty());
    }

    #[test]
    fn test_shared_values_chain_through_several_conditions() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build(
            "placed for {{team}}",
        )
        .with_for_some(vec!["replica".into(), "region".into()])
        .with_conditions(vec![
            String::from("{{replica}} is healthy"),
            String::from("{{replica}} runs in {{region}}"),
            String::from("{{region}} hosts {{team}}"),
        ])]));
        t.add_agent(Agent::build("infra").with_provides(vec![
            Behavior::build("east hosts blue"),
            Behavior::build("west hosts red"),
            Behavior::build("r1 runs in east"),
            Behavior::build("r2 runs in west"),
            Behavior::build("r1 is healthy"),
            Behavior::build("r2 is healthy"),
        ]));
        let r = t.resolve("placed for blue");
        assert_eq!(r.get_satisfying_offers().len(), 1);
        let conditions = r.get_satisfying_offers()[0].get_resolved_conditions();
        assert_eq!(conditions[0].get_behavior_name(), "r1 is healthy");
        assert_eq!(conditions[1].get_behavior_name(), "r1 runs in east");
        assert_eq!(conditions[2].get_behavior_name(), "east hosts blue");
        assert!(!t.resolve("placed for green").is_satisfied());
    }

    #[test]
    fn test_shared_value_on_a_loop_is_reported() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_for_some(vec!["thing".into()])
            .with_conditions(vec![
                String::from("will support {{thing}}"),
                String::from("{{thing}} does stuff"),
            ])]));
        t.add_agent(Agent::build("T1").with_provides(vec![
            Behavior::build("will support T1"),
            Behavior::build("T1 does stuff").with_conditions(vec![String::from("configured")]),
        ]));
        assert!(!t.resolve("configured").is_satisfied());
        assert_eq!(
            t.dependency_cycles(),
            vec![vec![String::from("T1 does stuff"), String::from("configured")]]
        );
    }

    #[test]
    fn test_open_goal_on_a_loop_terminates_and_is_reported() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a").with_provides(vec![Behavior::build("a is ready")
            .with_for_some(vec!["x".into()])
            .with_conditions(vec![String::from("{{x}} is ready")])]));
        assert!(!t.resolve("a is ready").is_satisfied());
        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["a is ready".to_string(), "{{x}} is ready".to_string()]]
        );
    }

    #[test]
    fn test_open_condition_passes_through_a_folded_collective() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("op").with_provides(vec![Behavior::build("configured")
            .with_for_some(vec!["doc".into()])
            .with_conditions(vec![String::from("{{doc}} shows usage")])]));
        t.add_agent(Agent::build("d1").with_provides(vec![Behavior::build("{{!self}} shows usage")]));
        t.add_superagent(
            SuperAgent::new(String::from("team"))
                .with_agent("op")
                .with_agent("d1"),
        );
        assert_eq!(t.get_working_agent_names(), vec!["team"]);
        // the condition is not reduced away; the collective meets it itself
        assert_eq!(
            t.resolve("configured"),
            Resolution::new("configured").add_satisfying_offer(Offer::new_conditional(
                "team",
                vec![Resolution::new("{{doc}} shows usage")
                    .add_satisfying_offer(Offer::new("team").answering("d1 shows usage"))]
            ))
        );
    }

    /// The spec's three agents: B keeps the gateway configured for some
    /// `kc` that the app deploys to and that deploys the app's gateway; C,
    /// parameterized, deploys any app's gateway.
    fn gateway(first: &str, second: &str, c_deploys: &str) -> Tracker {
        gateway_with(first, second, &format!("{{{{!self}}}} deploys {} gateway", c_deploys), &[])
    }

    fn gateway_with(first: &str, second: &str, c: &str, c_conditions: &[&str]) -> Tracker {
        let c_conditions: String = c_conditions
            .iter()
            .map(|x| format!("      - \"{}\"\n", x))
            .collect();
        let c_conditions = if c_conditions.is_empty() {
            String::new()
        } else {
            format!("    conditions:\n{}", c_conditions)
        };
        crate::marks::tracker(&format!(
            r#"kind: Agent
name: A
wants:
  - name: "sampleapp gateway is configured"
provides:
  - name: "sampleapp CD deployjob is run"
---
kind: Agent
name: B
provides:
  - name: "{{{{appname}}}} gateway is configured"
    forSome: [kc]
    conditions:
      - "{first}"
      - "{second}"
  - name: "{{{{appname}}}} deploys to {{{{kc}}}}"
    conditions:
      - "{{{{appname}}}} CD deployjob is run"
---
kind: Agent
name: C
provides:
  - name: "{c}"
{c_conditions}"#
        ))
    }

    const DEPLOYS_TO: &str = "{{appname}} deploys to {{kc}}";
    const DEPLOYS_GATEWAY: &str = "{{kc}} deploys {{appname}} gateway";

    #[test]
    fn test_declared_variable_is_found_through_a_parameterized_promise() {
        for (first, second) in [(DEPLOYS_TO, DEPLOYS_GATEWAY), (DEPLOYS_GATEWAY, DEPLOYS_TO)] {
            let t = gateway(first, second, "{{appname}}");
            let r = t.resolve("sampleapp gateway is configured");
            assert!(r.is_satisfied(), "{} / {}", first, second);
            // The only candidate is C: one offer, standing on concrete goals,
            // and nothing else.
            assert_eq!(r.get_satisfying_offers().len(), 1);
            assert!(r.get_unsatisfying_offers().is_empty());
            let offer = &r.get_satisfying_offers()[0];
            assert_eq!(offer.get_agent_name(), "B");
            let mut names: Vec<&str> = offer
                .get_resolved_conditions()
                .iter()
                .map(|c| c.get_behavior_name())
                .collect();
            names.sort();
            assert_eq!(names, vec!["C deploys sampleapp gateway", "sampleapp deploys to C"]);
            // The value `kc` took is marked where it went.
            let marked: Vec<String> = offer
                .get_resolved_conditions()
                .iter()
                .map(|c| c.marked_name())
                .collect();
            assert!(marked.contains(&"<sampleapp> deploys to <C>".to_string()), "{:?}", marked);
            assert!(t.dependency_cycles().is_empty());
        }
    }

    #[test]
    fn test_declared_variable_named_only_for_another_asker_is_unsatisfied() {
        // C deploys only otherapp's gateway: something names a value, not
        // for this asker.
        let t = gateway(DEPLOYS_TO, DEPLOYS_GATEWAY, "otherapp");
        let r = t.resolve("sampleapp gateway is configured");
        assert!(!r.is_satisfied());
        // C names a value, for otherapp only.
        assert!(!t.propose("{{kc}} deploys sampleapp gateway").names("kc"));
        assert!(t.propose("{{kc}} deploys otherapp gateway").names("kc"));
    }

    #[test]
    fn test_propose_reports_what_holds_for_any_value() {
        let t = gateway(DEPLOYS_TO, DEPLOYS_GATEWAY, "{{appname}}");
        // B's own `deploys to` names no value for `kc`.
        let p = t.propose("sampleapp deploys to {{kc}}");
        assert!(p.values.is_empty());
        assert_eq!(
            p.holding_for_any("kc"),
            vec![&HoldsForAny {
                variable: "kc".into(),
                agent: "B".into(),
                promise: "{{appname}} deploys to {{kc}}".into(),
            }]
        );
        // C's text names one.
        let p = t.propose("{{kc}} deploys sampleapp gateway");
        assert_eq!(p.values, vec![Bindings::from([("kc".to_string(), "C".to_string())])]);
        assert!(p.any.is_empty());
        // A variable lined up with a promise's variable is looked for in its
        // conditions: B's `deploys to` needs `{{appname}} CD deployjob is
        // run`, which A names for sampleapp.
        let p = t.propose("{{app}} deploys to C");
        assert_eq!(
            p.values,
            vec![Bindings::from([("app".to_string(), "sampleapp".to_string())])]
        );
    }

    /// O1 keeps `paired` for an `a` and `b` that pair up and both hold.
    fn pairs(pairs: &[(&str, &str)], ok: &[&str]) -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("paired")
            .with_for_some(vec!["a".into(), "b".into()])
            .with_conditions(vec![
                "{{a}} pairs with {{b}}".into(),
                "{{a}} is ok".into(),
                "{{b}} is ok".into(),
            ])]));
        t.add_agent(Agent::build("P").with_provides(
            pairs
                .iter()
                .map(|(a, b)| Behavior::build(&format!("{} pairs with {}", a, b)))
                .collect(),
        ));
        t.add_agent(Agent::build("K").with_provides(
            ok.iter().map(|x| Behavior::build(&format!("{} is ok", x))).collect(),
        ));
        t
    }

    fn offer_conditions(offer: &Offer) -> Vec<String> {
        offer
            .get_resolved_conditions()
            .iter()
            .map(|c| c.get_behavior_name().to_string())
            .collect()
    }

    #[test]
    fn test_shared_variables_give_one_offer_per_value_never_mixed() {
        // 1 pairs with 2 and 3 with 4. Every one is ok except 2, so 1/2
        // fails; 1 with 4 would hold, but nothing pairs them.
        let t = pairs(&[("1", "2"), ("3", "4")], &["1", "3", "4"]);
        let r = t.resolve("paired");
        assert!(r.is_satisfied());
        assert_eq!(r.get_satisfying_offers().len(), 1);
        assert_eq!(
            offer_conditions(&r.get_satisfying_offers()[0]),
            vec!["3 pairs with 4", "3 is ok", "4 is ok"]
        );
        // Every condition mentioning a variable proposes, and the proposals
        // are pooled: `{{a}} is ok` proposes 1, 3 and 4 for `a` whatever `b`
        // is. So other pairings are read too, each one value per variable
        // throughout its offer, and each not kept.
        let offers: Vec<Vec<String>> = r
            .get_satisfying_offers()
            .iter()
            .chain(r.get_unsatisfying_offers())
            .map(offer_conditions)
            .collect();
        assert!(offers.contains(&vec![
            "1 pairs with 2".to_string(),
            "1 is ok".to_string(),
            "2 is ok".to_string()
        ]));
        for names in &offers {
            let a = names[0].split(' ').next().unwrap();
            let b = names[0].rsplit(' ').next().unwrap();
            assert_eq!(names[1], format!("{} is ok", a));
            assert_eq!(names[2], format!("{} is ok", b));
        }
        // Readings are in the order of what they read (their conditions'
        // text, sorted), never of what the variables are called.
        let unkept: Vec<Vec<String>> =
            r.get_unsatisfying_offers().iter().map(offer_conditions).collect();
        let by_text = |o: &Vec<String>| {
            let mut t = o.clone();
            t.sort();
            t
        };
        let mut sorted = unkept.clone();
        sorted.sort_by_key(by_text);
        assert_eq!(unkept, sorted);
        // nothing pairs 1 with 4, so with 3 not ok nothing holds
        assert!(!pairs(&[("1", "2"), ("3", "4")], &["1", "4"]).resolve("paired").is_satisfied());
    }

    #[test]
    fn test_independent_declared_variables_are_gathered_rows_of_one_offer() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("reviewed")
            .with_for_some(vec!["doc".into(), "who".into()])
            .with_conditions(vec!["{{doc}} shows usage".into(), "{{who}} signed off".into()])]));
        t.add_agent(Agent::build("D").with_provides(vec![
            Behavior::build("d1 shows usage").with_conditions(vec!["d1 is approved".into()]),
            Behavior::build("d2 shows usage"),
        ]));
        t.add_agent(Agent::build("R").with_provides(vec![
            Behavior::build("r1 signed off"),
            Behavior::build("r2 signed off"),
        ]));
        let r = t.resolve("reviewed");
        // one offer, not one per (doc, who)
        assert_eq!(
            r,
            Resolution::new("reviewed").add_satisfying_offer(Offer::new_conditional(
                "O1",
                vec![
                    Resolution::new("{{doc}} shows usage")
                        .add_unsatisfying_offer(
                            Offer::new_conditional("D", vec![Resolution::new("d1 is approved")])
                                .answering("d1 shows usage")
                        )
                        .add_satisfying_offer(Offer::new("D").answering("d2 shows usage")),
                    Resolution::new("{{who}} signed off")
                        .add_satisfying_offer(Offer::new("R").answering("r1 signed off"))
                        .add_satisfying_offer(Offer::new("R").answering("r2 signed off")),
                ]
            ))
        );
        // what each match took is marked on its offer
        let doc = &r.get_satisfying_offers()[0].get_resolved_conditions()[0];
        assert_eq!(
            doc.marked_offer(&doc.get_satisfying_offers()[0]),
            "<d2> shows usage"
        );
    }

    #[test]
    fn test_gathered_row_with_no_match_is_unsatisfied_under_its_own_text() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("reviewed")
            .with_for_some(vec!["doc".into()])
            .with_conditions(vec!["{{doc}} shows usage".into()])]));
        assert_eq!(
            t.resolve("reviewed"),
            Resolution::new("reviewed").add_unsatisfying_offer(Offer::new_conditional(
                "O1",
                vec![Resolution::new("{{doc}} shows usage")]
            ))
        );
        assert!(t.dependency_cycles().is_empty());
    }

    #[test]
    fn test_promise_reaching_itself_through_a_variable_is_a_loop() {
        // (A gathered row reaching its own promise is
        // `test_open_goal_on_a_loop_terminates_and_is_reported`.) The
        // gateway, where C's deploy needs the gateway configured first: B
        // reaches itself through `kc = C`.
        let t = gateway_with(
            DEPLOYS_TO,
            DEPLOYS_GATEWAY,
            "{{!self}} deploys {{appname}} gateway",
            &["{{appname}} gateway is configured"],
        );
        assert!(!t.resolve("sampleapp gateway is configured").is_satisfied());
        assert!(t.dependency_cycles().contains(&vec![
            "C deploys sampleapp gateway".to_string(),
            "sampleapp gateway is configured".to_string()
        ]));

        // A parameterized promise answering its own row: `{{x}} is ready`
        // lines up with `{{n}}`, and none of the promise's conditions say
        // anything about `n`, so it holds for any value and names none. That
        // is reported as a variable that can be anything, not as a loop.
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("w")
                .with_wants(vec![Behavior::build("a is ready")]),
        );
        t.add_agent(Agent::build("p").with_provides(vec![Behavior::build("{{n}} is ready")
            .with_for_some(vec!["x".into()])
            .with_conditions(vec!["{{x}} is ready".into()])]));
        assert!(!t.resolve("a is ready").is_satisfied());
        assert!(t.dependency_cycles().is_empty());
        assert_eq!(t.unnamable_variables().len(), 1);
        assert_eq!(t.unnamable_variables()[0].variable, "x");

        // A search that comes back to its own question under other names
        // stops, and proposes nothing. No concrete goal is ever reached, so
        // there is no loop of behaviors to report: like any parameterized
        // loop, it is named only once something grounds it.
        let mut t = Tracker::new();
        t.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("{{a}} up").with_conditions(vec!["{{a}} down".into()]),
            Behavior::build("{{b}} down").with_conditions(vec!["{{b}} up".into()]),
            Behavior::build("r")
                .with_for_some(vec!["x".into()])
                .with_conditions(vec!["{{x}} up".into()]),
        ]));
        assert_eq!(t.propose("{{x}} up"), Proposals::default());
        assert!(!t.resolve("r").is_satisfied());
        assert!(t.dependency_cycles().is_empty());
    }

    /// `op` keeps `{{app}} is served` for a `node` the app runs on and that
    /// serves it. What `host` promises for `runs on` holds for any node.
    fn serving(serves: Option<&str>, runs_on: bool) -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("w")
                .with_wants(vec![Behavior::build("web is served")])
                .with_provides(vec![Behavior::build("web is built")]),
        );
        t.add_agent(Agent::build("op").with_provides(vec![Behavior::build("{{app}} is served")
            .with_for_some(vec!["node".into()])
            .with_conditions(vec![
                "{{app}} runs on {{node}}".into(),
                "{{node}} serves {{app}}".into(),
            ])]));
        if runs_on {
            t.add_agent(Agent::build("host").with_provides(vec![Behavior::build(
                "{{app}} runs on {{node}}",
            )
            .with_conditions(vec!["{{app}} is built".into()])]));
        }
        if let Some(app) = serves {
            t.add_agent(
                Agent::build("n1")
                    .with_provides(vec![Behavior::build(&format!("{{{{!self}}}} serves {}", app))]),
            );
        }
        t
    }

    /// `{{who}}/prod reads R1`, which takes the part before the slash from
    /// whoever asks.
    const PROD_READS: &str = r#"
kind: Agent
name: R
provides:
  - name: "{{who}}/prod reads R1"
"#;

    fn reads_prod(t: &Tracker, goal: &str) -> Resolution {
        let r = t.resolve(goal);
        assert!(r.is_satisfied(), "{:?}", r);
        r
    }

    #[test]
    fn test_a_value_filled_in_on_the_asking_side_is_not_wording() {
        // From `{{!self}}`.
        let t = crate::marks::tracker(&format!(
            r#"kind: Agent
name: web
provides:
  - name: "{{{{!self}}}} is served"
    forSome: [env]
    conditions:
      - "{{{{!self}}}}/{{{{env}}}} reads R1"
---{PROD_READS}"#
        ));
        let r = reads_prod(&t, "web is served");
        let row = &r.get_satisfying_offers()[0].get_resolved_conditions()[0];
        assert_eq!(row.get_satisfying_offers()[0].get_behavior(), Some("web/prod reads R1"));

        // From the promise name's binding.
        let t = crate::marks::tracker(&format!(
            r#"kind: Agent
name: B
provides:
  - name: "{{{{app}}}} is served"
    forSome: [env]
    conditions:
      - "{{{{app}}}}/{{{{env}}}} reads R1"
---{PROD_READS}"#
        ));
        reads_prod(&t, "web is served");

        // From an Instance binding.
        let t = crate::marks::tracker(&format!(
            r#"kind: Agent
name: base
provides:
  - name: "{{{{app}}}} ready"
    forSome: [env]
    conditions:
      - "{{{{app}}}}/{{{{env}}}} reads R1"
---
kind: Instance
name: i1
base: Agent/base
bindings:
  app: web
---{PROD_READS}"#
        ));
        reads_prod(&t, "web ready");

        // From a shared value: `region` is found first, and the row still
        // holding `x` has it filled in.
        let t = crate::marks::tracker(
            r#"kind: Agent
name: O
provides:
  - name: placed
    forSome: [region, x]
    conditions:
      - "{{region}} hosts db"
      - "{{region}} is open"
      - "{{region}}/{{x}} reads R1"
---
kind: Agent
name: I
provides:
  - name: east hosts db
  - name: east is open
  - name: "{{who}}/prod reads R1"
"#,
        );
        let r = reads_prod(&t, "placed");
        let row = &r.get_satisfying_offers()[0].get_resolved_conditions()[2];
        assert_eq!(row.get_behavior_name(), "east/{{x}} reads R1");
        assert_eq!(row.get_satisfying_offers()[0].get_behavior(), Some("east/prod reads R1"));

        // Written out by hand, the same text is wording, and nothing
        // matches it.
        let t = crate::marks::tracker(&format!(
            r#"kind: Agent
name: B
provides:
  - name: served
    forSome: [env]
    conditions:
      - "web/{{{{env}}}} reads R1"
---{PROD_READS}"#
        ));
        assert!(!t.resolve("served").is_satisfied());
    }

    #[test]
    fn test_answers_are_cached_by_what_was_filled_in_too() {
        // `web/{{env}} reads R1` twice under one goal: written out by hand
        // (wording, unmatched) and through `{{!self}}` (a value, matched).
        // Each keeps its own answer whichever is resolved first.
        for order in [["by-hand", "by-self"], ["by-self", "by-hand"]] {
            let t = crate::marks::tracker(&format!(
                r#"kind: Agent
name: top
provides:
  - name: both
    conditions: ["{}", "{}"]
---
kind: Agent
name: web
provides:
  - name: by-hand
    forSome: [env]
    conditions: ["web/{{{{env}}}} reads R1"]
  - name: by-self
    forSome: [env]
    conditions: ["{{{{!self}}}}/{{{{env}}}} reads R1"]
---{PROD_READS}"#,
                order[0], order[1]
            ));
            assert!(!t.resolve("by-hand").is_satisfied());
            assert!(t.resolve("by-self").is_satisfied());
            let both = t.resolve("both");
            let held: Vec<bool> = both.get_unsatisfying_offers()[0]
                .get_resolved_conditions()
                .iter()
                .map(|c| c.is_satisfied())
                .collect();
            let expected: Vec<bool> = order.iter().map(|o| *o == "by-self").collect();
            assert_eq!(held, expected, "{:?}", order);
        }
    }

    #[test]
    fn test_invalid_variable_is_reported_once_as_written_folded_or_not() {
        // be's `db` can be anything; fe's is named (`pg is fast`). Folded,
        // be's promise is hoisted into fe's and its `db` renamed there.
        for fold in [true, false] {
            let mut t = crate::marks::tracker(
                r#"kind: Agent
name: A
wants:
  - name: "web served"
---
kind: SuperAgent
name: team
agents: [fe, be]
---
kind: Agent
name: fe
provides:
  - name: "web served"
    forSome: [db]
    conditions:
      - "backend ready"
      - "{{db}} is fast"
---
kind: Agent
name: be
provides:
  - name: "backend ready"
    forSome: [db]
    conditions:
      - "{{db}} is primary"
---
kind: Agent
name: D
provides:
  - name: "{{x}} is primary"
  - name: "pg is fast"
"#,
            );
            t.set_fold_collectives(fold);
            let found = t.unnamable_variables();
            assert_eq!(found.len(), 1, "fold={} {:?}", fold, found);
            assert_eq!(
                (found[0].agent.as_str(), found[0].promise.as_str(), found[0].variable.as_str()),
                ("be", "backend ready", "db"),
                "fold={}",
                fold
            );
        }
    }

    /// fe keeps `web served` for a cluster that serves web, if the backend
    /// is ready; be keeps that for a database that is primary and
    /// replicated. 20 of each; only k19 and d19 hold.
    fn wide_team() -> Tracker {
        let mut yaml = String::from(
            r#"kind: Agent
name: A
wants:
  - name: "web served"
---
kind: SuperAgent
name: team
agents: [fe, be]
---
kind: Agent
name: fe
provides:
  - name: "web served"
    forSome: [kc]
    conditions:
      - "backend ready"
      - "{{kc}} is a cluster"
      - "{{kc}} serves web"
---
kind: Agent
name: be
provides:
  - name: "backend ready"
    forSome: [db]
    conditions:
      - "{{db}} is primary"
      - "{{db}} is replicated"
---
kind: Agent
name: D
provides:
  - name: "yes"
"#,
        );
        for i in 0..20 {
            let held = if i == 19 { "yes" } else { "nope" };
            yaml.push_str(&format!(
                "  - name: \"k{i:02} is a cluster\"\n  - name: \"d{i:02} is primary\"\n  - name: \"d{i:02} is replicated\"\n    conditions: [\"{held}\"]\n"
            ));
        }
        yaml.push_str("  - name: \"k19 serves web\"\n");
        crate::marks::tracker(&yaml)
    }

    #[test]
    fn test_independent_variables_do_not_multiply_against_the_cap() {
        // Folded, be's `db` is hoisted into fe's promise beside `kc`: 20 x
        // 20 combinations, more than the cap, but nothing ties the two.
        let mut t = wide_team();
        let folded = t.resolve("web served");
        assert!(folded.is_satisfied());
        assert_eq!(folded.get_satisfying_offers().len(), 1);
        t.set_fold_collectives(false);
        assert!(t.resolve("web served").is_satisfied());
        // Every group's readings were resolved; only combinations that are
        // not kept were left out of the offers, so nothing was cut short.
        assert!(!folded.is_cut_short());
        assert!(folded.get_unsatisfying_offers().len() < 400);
    }

    /// `go` for an `a` and `b` that pair up, 300 values of `a`.
    fn pairs_over_the_cap(conditions: [&str; 3]) -> Tracker {
        let mut yaml = format!(
            "kind: Agent\nname: A\nwants:\n  - name: go\nprovides:\n  - name: go\n    forSome: [a, b]\n    conditions: [\"{}\", \"{}\", \"{}\"]\n---\nkind: Agent\nname: Z\nprovides:\n",
            conditions[0], conditions[1], conditions[2]
        );
        for i in 0..300 {
            yaml.push_str(&format!("  - name: \"a{i:03} is A\"\n"));
        }
        yaml.push_str("  - name: \"b0 is B\"\n  - name: \"a299 with b0\"\n");
        crate::marks::tracker(&yaml)
    }

    #[test]
    fn test_the_cap_does_not_depend_on_condition_order() {
        let orders = [
            ["{{a}} is A", "{{b}} is B", "{{a}} with {{b}}"],
            ["{{a}} with {{b}}", "{{b}} is B", "{{a}} is A"],
            ["{{b}} is B", "{{a}} is A", "{{a}} with {{b}}"],
        ];
        for order in orders {
            let r = pairs_over_the_cap(order).resolve("go");
            // The one condition that names the pair names it complete, so
            // it is read before the 300 partial ones fill the cap.
            assert!(r.is_satisfied(), "{:?}", order);
            // Kept, so certain: nothing the cap left out could change it.
            assert!(!r.is_cut_short(), "{:?}", order);
        }
    }

    /// `go` for an `a` that is A and is good: 300 are A, only the last is
    /// good, and every one is a complete reading of its own.
    fn good_over_the_cap(first: &str, second: &str) -> Tracker {
        let mut yaml = format!(
            "kind: Agent\nname: A\nwants:\n  - name: go\nprovides:\n  - name: go\n    forSome: [a]\n    conditions: [\"{first}\", \"{second}\"]\n---\nkind: Agent\nname: Z\nprovides:\n"
        );
        for i in 0..300 {
            yaml.push_str(&format!("  - name: \"a{i:03} is A\"\n"));
        }
        yaml.push_str("  - name: \"a299 is good\"\n");
        crate::marks::tracker(&yaml)
    }

    #[test]
    fn test_a_joint_group_over_the_cap_is_reported_cut_short() {
        // One genuinely joint group of 300 readings: the first 256 by value
        // are tried, a299 is not, and the answer says so, the same in
        // either order.
        for (first, second) in [("{{a}} is A", "{{a}} is good"), ("{{a}} is good", "{{a}} is A")] {
            let t = good_over_the_cap(first, second);
            let r = t.resolve("go");
            assert!(!r.is_satisfied());
            assert!(r.is_cut_short());
            assert_eq!(r.get_unsatisfying_offers().len(), MAX_CONDITION_READINGS);
            let causes = crate::query::unsatisfied(&t, crate::query::QueryOptions::default());
            // said first, before the causes as far as resolution went
            assert_eq!(
                causes[0].causes[0],
                crate::query::Cause {
                    behavior: "go".into(),
                    kind: crate::query::CauseKind::CutShort,
                    needed_by: None,
                }
            );
        }
    }

    #[test]
    fn test_cut_short_shows_through_a_gathered_row() {
        // `p is go` is cut short; `go` reaches it through `{{x}} is go`.
        let mut yaml = String::from(
            "kind: Agent\nname: A\nwants:\n  - name: go\nprovides:\n  - name: go\n    forSome: [x]\n    conditions: [\"{{x}} is go\"]\n---\nkind: Agent\nname: Z\nprovides:\n  - name: p is go\n    forSome: [a]\n    conditions: [\"{{a}} is A\", \"{{a}} is good\"]\n",
        );
        for i in 0..257 {
            yaml.push_str(&format!("  - name: \"a{i:03} is A\"\n"));
        }
        yaml.push_str("  - name: \"a256 is good\"\n");
        let t = crate::marks::tracker(&yaml);
        let r = t.resolve("go");
        assert!(!r.is_satisfied());
        let row = &r.get_unsatisfying_offers()[0].get_resolved_conditions()[0];
        assert_eq!(row.get_behavior_name(), "{{x}} is go");
        assert!(row.is_cut_short());
        let rows = crate::query::unsatisfied(&t, crate::query::QueryOptions::default());
        assert_eq!(rows[0].causes[0].kind, crate::query::CauseKind::CutShort);
    }

    /// `b` likes `o`, 300 pairs, `b000 likes c299` the only one where `b`
    /// is good and `o` is ok. Sorted by `b`'s values that pair comes
    /// first; sorted by `o`'s, last, beyond the cap.
    fn liking(b: &str, o: &str) -> String {
        let mut yaml = format!(
            "    forSome: [{b}, {o}]\n    conditions: [\"{{{{{b}}}}} likes {{{{{o}}}}}\", \"{{{{{b}}}}} is good\", \"{{{{{o}}}}} is ok\"]\n---\nkind: Agent\nname: Z\nprovides:\n  - name: b000 is good\n  - name: c299 is ok\n  - name: f1 is front\n"
        );
        for i in 0..300 {
            yaml.push_str(&format!("  - name: \"b{i:03} likes c{:03}\"\n", 299 - i));
        }
        yaml
    }

    #[test]
    fn test_renaming_a_variable_does_not_change_what_the_cap_keeps() {
        for (b, o) in [("b", "c"), ("z", "c"), ("b", "a"), ("q", "b0")] {
            let t = crate::marks::tracker(&format!(
                "kind: Agent\nname: A\nwants:\n  - name: ready\nprovides:\n  - name: ready\n{}",
                liking(b, o)
            ));
            let r = t.resolve("ready");
            assert!(r.is_satisfied(), "{} {}", b, o);
            assert!(!r.is_cut_short(), "{} {}", b, o);
        }
    }

    #[test]
    fn test_folding_renames_without_changing_what_the_cap_keeps() {
        // be declares `b` and `b0`; folded into fe's promise, `b` is
        // renamed `b_2` because fe has its own `b`. Folded and unfolded
        // must agree.
        let yaml = format!(
            r#"kind: Agent
name: A
wants:
  - name: "web served"
---
kind: SuperAgent
name: team
agents: [fe, be]
---
kind: Agent
name: fe
provides:
  - name: "web served"
    forSome: [b]
    conditions: ["backend ready", "{{{{b}}}} is front"]
---
kind: Agent
name: be
provides:
  - name: "backend ready"
{}"#,
            liking("b", "b0")
        );
        let mut t = crate::marks::tracker(&yaml);
        let hoisted = t.get_agent_promises("team").unwrap();
        assert!(hoisted
            .iter()
            .any(|p| p.get_for_some().contains(&"b_2".to_string())));
        assert!(t.resolve("web served").is_satisfied());
        t.set_fold_collectives(false);
        assert!(t.resolve("web served").is_satisfied());
    }

    #[test]
    fn test_a_group_failing_outright_makes_the_answer_certain() {
        // `a` has 257 readings, one more than the cap, and only the last
        // is good; but `missing thing` is a group of its own, resolved and
        // failing, so nothing the cap left out could keep `go`.
        let build = |missing: bool| {
            let mut yaml = format!(
                "kind: Agent\nname: A\nwants:\n  - name: go\nprovides:\n  - name: go\n    forSome: [a]\n    conditions: [\"{{{{a}}}} is A\", \"{{{{a}}}} is good\"{}]\n---\nkind: Agent\nname: Z\nprovides:\n",
                if missing { ", \"missing thing\"" } else { "" }
            );
            for i in 0..257 {
                yaml.push_str(&format!("  - name: \"a{i:03} is A\"\n"));
            }
            yaml.push_str("  - name: \"a256 is good\"\n");
            crate::marks::tracker(&yaml)
        };
        let t = build(true);
        let r = t.resolve("go");
        assert!(!r.is_satisfied());
        assert!(!r.is_cut_short());
        let causes = &crate::query::unsatisfied(&t, crate::query::QueryOptions::default())[0].causes;
        assert!(causes.iter().all(|c| c.kind != crate::query::CauseKind::CutShort));
        assert!(causes.iter().any(|c| c.behavior == "missing thing"));
        // Without it, the one reading that could keep `go` was never tried.
        assert!(build(false).resolve("go").is_cut_short());
    }

    /// `w` for an `x` and a `y`, each needing two things: 20 values each,
    /// `q1` and `q2` provided for none, so 400 combinations, none kept.
    fn combos(x_first: bool) -> Tracker {
        let x = "\"{{x}} r1\", \"{{x}} q1\"";
        let y = "\"{{y}} r2\", \"{{y}} q2\"";
        let (first, second) = if x_first { (x, y) } else { (y, x) };
        let mut yaml = format!(
            "kind: Agent\nname: A\nwants:\n  - name: w\nprovides:\n  - name: w\n    forSome: [x, y]\n    conditions: [{first}, {second}]\n---\nkind: Agent\nname: Z\nprovides:\n"
        );
        for i in 1..=20 {
            yaml.push_str(&format!("  - name: \"a{i:02} r1\"\n  - name: \"b{i:02} r2\"\n"));
        }
        crate::marks::tracker(&yaml)
    }

    #[test]
    fn test_offers_shown_over_the_cap_cover_every_reading_in_any_order() {
        let summary = |t: &Tracker| -> Vec<String> {
            let rows = crate::query::unsatisfied(t, crate::query::QueryOptions::default());
            rows[0].causes.iter().map(|c| c.behavior.clone()).collect()
        };
        let (xy, yx) = (combos(true), combos(false));
        let r = xy.resolve("w");
        assert_eq!(r.get_unsatisfying_offers().len(), MAX_CONDITION_READINGS);
        assert!(!r.is_cut_short());
        // Every reading of both groups is in some offer shown, so every
        // cause is listed: 20 `q1` and 20 `q2`.
        let causes = summary(&xy);
        assert_eq!(causes.len(), 40, "{:?}", causes);
        assert_eq!(causes, summary(&yx));
        // and the provides listing reads the same, condition by condition
        let held = |t: &Tracker| -> BTreeSet<(String, bool)> {
            crate::query::agent_provides(t, "A", crate::query::QueryOptions::default())
                .unwrap()
                .provides[0]
                .conditions
                .iter()
                .map(|c| (c.behavior.clone(), c.held))
                .collect()
        };
        assert_eq!(held(&xy), held(&yx));
    }

    #[test]
    fn test_combinations_left_out_are_not_cut_short() {
        // Two independent groups, 17 and 16 readings: every reading is
        // resolved, so the answer is certain whatever the cap leaves out
        // of the offers shown.
        let build = |n: usize| {
            let mut yaml = String::from(
                "kind: Agent\nname: A\nwants:\n  - name: go\nprovides:\n  - name: go\n    forSome: [a, b]\n    conditions: [\"{{a}} is A\", \"{{a}} is agood\", \"{{b}} is B\", \"{{b}} is bgood\"]\n---\nkind: Agent\nname: Z\nprovides:\n",
            );
            for i in 0..n {
                yaml.push_str(&format!("  - name: \"a{i:03} is A\"\n"));
            }
            for i in 0..16 {
                yaml.push_str(&format!("  - name: \"b{i:03} is B\"\n"));
            }
            yaml.push_str("  - name: \"b015 is bgood\"\n");
            crate::marks::tracker(&yaml)
        };
        for n in [16, 17] {
            let r = build(n).resolve("go");
            assert!(!r.is_satisfied());
            assert!(!r.is_cut_short(), "n={}", n);
        }
    }

    #[test]
    fn test_gateway_without_c_is_invalid() {
        let mut t = gateway(DEPLOYS_TO, DEPLOYS_GATEWAY, "{{appname}}");
        assert!(t.unnamable_variables().is_empty());
        t = crate::marks::tracker(
            r#"kind: Agent
name: A
wants:
  - name: "sampleapp gateway is configured"
provides:
  - name: "sampleapp CD deployjob is run"
---
kind: Agent
name: B
provides:
  - name: "{{appname}} gateway is configured"
    forSome: [kc]
    conditions:
      - "{{appname}} deploys to {{kc}}"
      - "{{kc}} deploys {{appname}} gateway"
  - name: "{{appname}} deploys to {{kc}}"
    conditions:
      - "{{appname}} CD deployjob is run"
"#,
        );
        let found = t.unnamable_variables();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].to_string(),
            "`kc` can be anything: B's `{{appname}} deploys to {{kc}}` holds for any value and nothing names one"
        );
        // and it is unsatisfied, with nothing found: the conditions as written
        assert_eq!(
            t.resolve("sampleapp gateway is configured"),
            Resolution::new("sampleapp gateway is configured").add_unsatisfying_offer(
                Offer::new_conditional(
                    "B",
                    vec![
                        Resolution::new("sampleapp deploys to {{kc}}"),
                        Resolution::new("{{kc}} deploys sampleapp gateway"),
                    ]
                )
            )
        );
    }

    #[test]
    fn test_a_variable_only_a_holds_for_any_promise_answers_is_invalid() {
        let t = serving(None, true);
        let found = t.unnamable_variables();
        assert_eq!(
            found,
            vec![UnnamableVariable {
                agent: "op".into(),
                promise: "{{app}} is served".into(),
                variable: "node".into(),
                holders: vec![HoldsForAny {
                    variable: "node".into(),
                    agent: "host".into(),
                    promise: "{{app}} runs on {{node}}".into(),
                }],
            }]
        );
        assert_eq!(
            found[0].to_string(),
            "`node` can be anything: host's `{{app}} runs on {{node}}` holds for any value and nothing names one"
        );
        assert!(!t.resolve("web is served").is_satisfied());

        // Something names a value, though not for this asker: unsatisfied,
        // not invalid.
        let t = serving(Some("api"), true);
        assert!(t.unnamable_variables().is_empty());
        assert!(!t.resolve("web is served").is_satisfied());
        // and for this one, satisfied
        let t = serving(Some("web"), true);
        assert!(t.unnamable_variables().is_empty());
        assert!(t.resolve("web is served").is_satisfied());

        // Nothing matches the conditions at all: unsatisfied, not invalid.
        let t = serving(None, false);
        assert!(t.unnamable_variables().is_empty());
        assert!(!t.resolve("web is served").is_satisfied());
    }

    #[test]
    fn test_folded_collective_keeps_an_inner_declared_variable() {
        // `site` folds `op` and `web`: `served` rests on `web runs`, met
        // inside, which needs some node it is placed on that is up.
        let build = |up: &str| {
            let mut t = Tracker::new();
            t.add_agent(Agent::build("op").with_provides(vec![
                Behavior::build("served").with_conditions(vec!["web runs".into()]),
            ]));
            t.add_agent(Agent::build("web").with_provides(vec![Behavior::build("web runs")
                .with_for_some(vec!["node".into()])
                .with_conditions(vec![
                    "web placed on {{node}} node".into(),
                    "{{node}} node is up".into(),
                ])]));
            t.add_superagent(
                SuperAgent::new(String::from("site"))
                    .with_agent("op")
                    .with_agent("web"),
            );
            t.add_agent(Agent::build("infra").with_provides(vec![
                Behavior::build("web placed on n1 node"),
                Behavior::build(&format!("{} node is up", up)),
            ]));
            t
        };
        let t = build("n1");
        assert_eq!(t.get_working_agent_names(), vec!["infra", "site"]);
        let served = t
            .get_agent_promises("site")
            .unwrap()
            .into_iter()
            .find(|p| p.get_name() == "served")
            .unwrap();
        assert_eq!(served.get_for_some(), &["node".to_string()]);
        let r = t.resolve("served");
        assert!(r.is_satisfied());
        assert_eq!(
            offer_conditions(&r.get_satisfying_offers()[0]),
            vec!["web placed on n1 node", "n1 node is up"]
        );
        // n2 is up, but web is placed on n1
        assert!(!build("n2").resolve("served").is_satisfied());
    }

    #[test]
    fn test_declared_variables_judged_together_in_the_provides_listing() {
        let t = gateway(DEPLOYS_TO, DEPLOYS_GATEWAY, "{{appname}}");
        let readings = t.resolve_conditions(
            &Behavior::build("sampleapp gateway is configured")
                .with_for_some(vec!["kc".to_string()])
                .with_conditions(vec![
                    "sampleapp deploys to {{kc}}".to_string(),
                    "{{kc}} deploys sampleapp gateway".to_string(),
                ]),
        );
        assert_eq!(readings.len(), 1);
        assert!(readings[0].iter().all(|c| c.is_satisfied()));
        assert_eq!(readings[0][0].get_behavior_name(), "sampleapp deploys to C");
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

    /// a1 keeps b1 on b2, which a2 promises; both sit in sa1, and c wants b1.
    fn internal_promise_tracker() -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        t.add_agent(Agent::build("c").with_wants(vec![Behavior::build("b1")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        t
    }

    fn working(t: &Tracker) -> Vec<String> {
        let mut names: Vec<String> = t.get_working_agent_names().into_iter().cloned().collect();
        names.sort();
        names
    }

    #[test]
    fn test_collectives_fold_by_default() {
        let t = internal_promise_tracker();
        assert!(t.folds_collectives());
        assert_eq!(working(&t), vec!["c", "sa1"]);
        // The internal condition is reduced away inside the collective.
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("sa1"))
        );
    }

    #[test]
    fn test_unfolded_collective_members_stand_on_their_own() {
        let mut t = internal_promise_tracker();
        t.set_fold_collectives(false);
        assert!(!t.folds_collectives());
        assert_eq!(working(&t), vec!["a1", "a2", "c"]);
        assert!(!t.has_agent(String::from("sa1")));
        // The promise between members is an ordinary one now.
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new_conditional(
                "a1",
                vec![Resolution::new("b2").add_satisfying_offer(Offer::new("a2"))],
            ))
        );
        // And back again.
        t.set_fold_collectives(true);
        assert_eq!(working(&t), vec!["c", "sa1"]);
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("sa1"))
        );
    }

    #[test]
    fn test_unfold_set_before_loading_matches_unfold_after() {
        let mut before = Tracker::new();
        before.set_fold_collectives(false);
        before.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        before.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![String::from("b2")]),
        ]));
        before.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b2")]));
        before.add_agent(Agent::build("c").with_wants(vec![Behavior::build("b1")]));

        let mut after = internal_promise_tracker();
        after.set_fold_collectives(false);
        assert_eq!(working(&before), working(&after));
        assert_eq!(before.resolve("b1"), after.resolve("b1"));
    }

    #[test]
    fn test_unfolded_nested_collectives_unfold_all_the_way() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w1").with_wants(vec![Behavior::build("w")]));
        t.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("w").with_conditions(vec![String::from("wa1")]),
        ]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("wa1")]));
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
        t.set_fold_collectives(false);
        assert_eq!(working(&t), vec!["a1", "a2", "a3", "w1"]);
        assert_eq!(
            t.resolve("w"),
            Resolution::new("w").add_satisfying_offer(Offer::new_conditional(
                "a1",
                vec![Resolution::new("wa1").add_satisfying_offer(Offer::new("a2"))],
            ))
        );
    }

    #[test]
    fn test_unfolded_instance_still_copies_its_whole_collective() {
        let mut t = internal_promise_tracker();
        // sa1 is copied by i1, so it and its members stay templates; sa2 is
        // copied by nothing and unfolds.
        t.add_instance(Instance::new("i1", "SuperAgent/sa1"));
        t.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b3")]));
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("a3"));
        t.set_fold_collectives(false);

        assert_eq!(working(&t), vec!["a3", "c", "i1"]);
        // The copy is the folded collective, internal condition and all.
        assert_eq!(
            t.resolve("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("i1"))
        );
        assert_eq!(
            t.resolve("b3"),
            Resolution::new("b3").add_satisfying_offer(Offer::new("a3"))
        );
    }

    #[test]
    fn test_unfolded_membership_loop_leaves_its_agents_standing() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("w")]));
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
        t.set_fold_collectives(false);
        assert_eq!(working(&t), vec!["a1", "a2"]);
    }

    /// a1 is in sa1, which is only copied, and in sa2, which is not: sa2's
    /// share of a1 must not vanish when unfolded.
    #[test]
    fn test_unfold_keeps_an_agent_another_collective_holds() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("x")]));
        t.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("y")]));
        t.add_superagent(
            SuperAgent::new(String::from("sa1"))
                .with_agent("a1")
                .with_agent("a2"),
        );
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("a1"));
        t.add_instance(Instance::new("i1", "SuperAgent/sa1"));

        let offers = |t: &Tracker, b: &str| -> Vec<String> {
            let r = t.resolve(b);
            let mut names: Vec<String> = r
                .get_satisfying_offers()
                .iter()
                .chain(r.get_unsatisfying_offers())
                .map(|o| o.get_agent_name().to_string())
                .collect();
            names.sort();
            names
        };
        assert_eq!(offers(&t, "x"), vec!["i1", "sa2"]);
        assert_eq!(offers(&t, "y"), vec!["i1"]);

        t.set_fold_collectives(false);
        // a1 stands, for sa2; a2 is reached only through the copy.
        assert_eq!(working(&t), vec!["a1", "i1"]);
        assert_eq!(offers(&t, "x"), vec!["a1", "i1"]);
        assert_eq!(offers(&t, "y"), vec!["i1"]);
    }

    /// An agent that is a plain instance base but also sits in a collective
    /// stands unfolded, as it counts inside that collective folded.
    #[test]
    fn test_unfold_keeps_an_instance_base_another_collective_holds() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_provides(vec![Behavior::build("x")]));
        t.add_superagent(SuperAgent::new(String::from("sa2")).with_agent("a1"));
        t.add_instance(Instance::new("i1", "Agent/a1"));
        t.set_fold_collectives(false);
        assert_eq!(working(&t), vec!["a1", "i1"]);
    }

    #[test]
    fn test_folding_collectives_names_the_outermost_collective() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a"));
        t.add_agent(Agent::build("b"));
        t.add_agent(Agent::build("loose"));
        t.add_superagent(SuperAgent::new(String::from("inner")).with_agent("a"));
        t.add_superagent(
            SuperAgent::new(String::from("outer"))
                .with_agent("inner")
                .with_agent("b"),
        );
        t.add_superagent(SuperAgent::new(String::from("other")).with_agent("b"));
        assert_eq!(t.folding_collectives("a"), vec!["outer"]);
        assert_eq!(t.folding_collectives("b"), vec!["other", "outer"]);
        assert!(t.folding_collectives("loose").is_empty());
        // Unfolded, the answer is the same: it says where folding would go.
        t.set_fold_collectives(false);
        assert_eq!(t.folding_collectives("a"), vec!["outer"]);
    }

    #[test]
    fn test_folding_collectives_skips_a_collective_only_copied() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a"));
        t.add_superagent(SuperAgent::new(String::from("sa")).with_agent("a"));
        t.add_instance(Instance::new("i", "SuperAgent/sa"));
        assert!(t.folding_collectives("a").is_empty());
    }

    #[test]
    fn test_get_agent_promises_and_has_provider() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a").with_provides(vec![
            Behavior::build("x").with_conditions(vec![String::from("y")]),
            Behavior::build("db-{{n}}"),
        ]));
        let promises = t.get_agent_promises("a").unwrap();
        assert_eq!(promises.len(), 2);
        assert_eq!(promises[0].get_conditions(), vec!["y"]);
        assert_eq!(t.get_agent_promises("nobody"), None);
        assert!(t.has_provider("x"));
        assert!(t.has_provider("db-main"));
        assert!(!t.has_provider("y"));
    }

    // --- Fold: Unneeded alternatives ---------------------------------------

    fn conditional(name: &str, conditions: &[&str]) -> Behavior {
        Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
    }

    #[test]
    fn test_unneeded_alternatives_fold_inside_an_unsatisfied_chain() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w").with_wants(vec![Behavior::build("top")]));
        t.add_agent(Agent::build("p").with_provides(vec![conditional("top", &["z1", "z2"])]));
        t.add_agent(Agent::build("az1").with_provides(vec![Behavior::build("z1")]));
        t.add_agent(Agent::build("ay1").with_provides(vec![conditional("z1", &["y1"])]));

        let full = t.resolve("top");
        assert_eq!(
            full.to_strings_compressed(false),
            vec![
                "top |-> p &-> z1 |-> az1",
                "                 |-> ay1 &-> y1 |-> ?",
                "          &-> z2 |-> ?",
            ]
        );
        let pruned = full.without_unneeded_alternatives();
        assert!(!pruned.is_satisfied());
        assert_eq!(
            pruned,
            Resolution::new("top").add_unsatisfying_offer(Offer::new_conditional(
                "p",
                vec![
                    Resolution::new("z1").add_satisfying_offer(Offer::new("az1")),
                    Resolution::new("z2"),
                ]
            ))
        );
        assert_eq!(
            pruned.to_strings_compressed(false),
            vec!["top |-> p &-> z1 |-> az1", "          &-> z2 |-> ?"]
        );
    }

    /// `w is fine` when someone is up; the row for `{{who}} is up`.
    fn who_is_up(t: &Tracker) -> Resolution {
        let r = t.resolve("w is fine");
        r.get_satisfying_offers()
            .iter()
            .chain(r.get_unsatisfying_offers())
            .next()
            .unwrap()
            .get_resolved_conditions()[0]
            .clone()
    }

    fn fine_if_someone_is_up() -> Agent {
        Agent::build("w")
            .with_wants(vec![Behavior::build("w is fine")])
            .with_provides(vec![Behavior::build("w is fine")
                .with_for_some(vec!["who".into()])
                .with_conditions(vec!["{{who}} is up".into()])])
    }

    #[test]
    fn test_unneeded_alternatives_fold_among_open_variable_candidates() {
        let mut t = Tracker::new();
        t.add_agent(fine_if_someone_is_up());
        t.add_agent(Agent::build("a").with_provides(vec![Behavior::build("a is up")]));
        t.add_agent(Agent::build("b").with_provides(vec![conditional("b is up", &["power"])]));

        let full = who_is_up(&t);
        assert_eq!(
            full.to_strings_compressed(false),
            vec![
                "{{who}} is up |-> a",
                "              |-> b &-> power |-> ?",
            ]
        );
        let pruned = full.without_unneeded_alternatives();
        assert_eq!(
            pruned,
            Resolution::new("{{who}} is up")
                .add_satisfying_offer(Offer::new("a").answering("a is up"))
        );
        assert_eq!(
            pruned.to_strings_compressed(false),
            vec!["{{who}} is up |-> a"]
        );
    }

    #[test]
    fn test_unneeded_alternatives_keep_every_candidate_when_none_holds() {
        let mut t = Tracker::new();
        t.add_agent(fine_if_someone_is_up());
        t.add_agent(Agent::build("a").with_provides(vec![conditional("a is up", &["fuel"])]));
        t.add_agent(Agent::build("b").with_provides(vec![conditional("b is up", &["power"])]));

        let full = who_is_up(&t);
        let pruned = full.clone().without_unneeded_alternatives();
        assert_eq!(pruned, full);
        assert_eq!(
            pruned.to_strings_compressed(false),
            full.to_strings_compressed(false)
        );
        assert_eq!(
            pruned.to_strings_compressed(false),
            vec![
                "{{who}} is up |-> a &-> fuel |-> ?",
                "              |-> b &-> power |-> ?",
            ]
        );
    }

    #[test]
    fn test_unneeded_alternatives_fold_a_cycle_away() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("x").with_wants(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("A").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("B").with_provides(vec![conditional("b1", &["c"])]));
        t.add_agent(Agent::build("C").with_provides(vec![conditional("c", &["b1"])]));

        let full = t.resolve("b1");
        assert!(full.is_satisfied());
        assert_eq!(full.get_unsatisfying_offers().len(), 1);
        assert_eq!(
            full.without_unneeded_alternatives(),
            Resolution::new("b1").add_satisfying_offer(Offer::new("A"))
        );
    }

    #[test]
    fn test_unneeded_alternatives_keep_how_a_want_was_written() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("{{!self}} reads r1")]));
        t.add_agent(Agent::build("p").with_provides(vec![
            Behavior::build("a1 reads r1"),
            conditional("a1 reads r1", &["nothing"]),
        ]));
        let pruned = t
            .resolve_want("a1", "a1 reads r1")
            .without_unneeded_alternatives();
        assert_eq!(pruned.marked_name(), "<a1> reads r1");
        assert!(pruned.get_unsatisfying_offers().is_empty());
    }
}
