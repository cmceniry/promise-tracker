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
                    // An open variable is no gap: resolution finds its value.
                    if want.get_name_pattern().needs_binding() {
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
        // An open goal has a provider when anything it matches does.
        if let Some(matches) = self.open_matches(goal) {
            return !matches.is_empty();
        }
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

        // An open goal depends on everything it matches.
        if let Some(matches) = self.open_matches(goal) {
            for name in matches {
                complete &= self.walk_for_cycles(&name, in_progress, clean, found);
            }
            in_progress.pop();
            if complete {
                clean.insert(goal.to_string());
            }
            return complete;
        }

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
                    for reading in self.condition_readings(&conditions) {
                        for condition in reading {
                            let sub_goal = match condition {
                                ConditionGoal::Goal(sub_goal) => sub_goal.source().clone(),
                                ConditionGoal::Chosen { name, .. } => name,
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

        // An open goal is kept by whatever keeps any behavior it matches. Each
        // match is resolved as the concrete goal it is, and its offers are
        // gathered here under that name.
        if let Some(matches) = self.open_matches(goal) {
            let pattern = Pattern::parse_lossy(goal);
            for name in matches {
                let (resolved, stands_alone) = self.resolve_goal(&name, in_progress, settled);
                self_contained &= stands_alone;
                // Where the goal's variables fell in this match, for display.
                let matched = pattern.spans_on(&name).unwrap_or_default();
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
            in_progress.pop();
            if self_contained {
                settled.insert(goal.to_string(), r.clone());
            }
            return (r, self_contained);
        }

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
                    for reading in self.condition_readings(&conditions) {
                        let (resolved_conditions, stands_alone) =
                            self.resolve_reading(reading, in_progress, settled);
                        self_contained &= stands_alone;
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

        in_progress.pop();
        if self_contained {
            settled.insert(goal.to_string(), r.clone());
        }
        (r, self_contained)
    }
}

impl Tracker {
    /// The concrete behaviors an open goal could be answered by: every
    /// concrete name a working agent promises that the goal matches, by agent
    /// then declaration order, each once. None when the goal is not open.
    ///
    /// Only names promised outright are considered. A parameterized promise
    /// is not matched: that is a pattern on both sides, the general
    /// unification this design leaves alone. And only what is declared — never
    /// what another resolution happened to derive — so the answer does not
    /// depend on what was resolved first.
    pub fn open_matches(&self, goal: &str) -> Option<Vec<String>> {
        let pattern = Pattern::parse_lossy(goal);
        if !pattern.has_open() {
            return None;
        }
        let mut agent_names: Vec<&String> = self.working_agents.keys().collect();
        agent_names.sort();
        let mut matches: Vec<String> = vec![];
        for agent_name in agent_names {
            for variant in &self.working_agents[agent_name] {
                for promise in variant.provides() {
                    let name = promise.get_name_pattern();
                    if name.is_ground()
                        && pattern.match_ground(name.source()).is_some()
                        && !matches.contains(name.source())
                    {
                        matches.push(name.source().clone());
                    }
                }
            }
        }
        Some(matches)
    }
}

/// How many readings one promise's conditions may be given. Each condition
/// that shares a value multiplies them by its matches, so a contract can ask
/// for far more than is worth resolving.
const MAX_CONDITION_READINGS: usize = 256;

/// How one condition is to be resolved under one reading of its promise.
enum ConditionGoal {
    /// Resolve this goal as it stands.
    Goal(Pattern),
    /// An open condition whose value its siblings share, answered here by the
    /// one concrete behavior `name`.
    Chosen { pattern: Pattern, name: String },
    /// Still holding a variable nothing supplied a value for.
    Unbound(Pattern),
}

/// A goal's resolution, as the condition or want `reached` came to it: the
/// parts of the name a substitution filled in, and under an open goal where
/// those and its `{{?x}}` fall in each behavior matched. Display only; the
/// offers are untouched.
fn as_reached_by(mut resolved: Resolution, reached: &Pattern) -> Resolution {
    if reached.has_open() {
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
    pub fn resolve_conditions(&self, conditions: &[String]) -> Vec<Vec<Resolution>> {
        let patterns: Vec<Pattern> = conditions.iter().map(|c| Pattern::parse_lossy(c)).collect();
        let mut settled: HashMap<String, Resolution> = HashMap::new();
        self.condition_readings(&patterns)
            .into_iter()
            .map(|reading| self.resolve_reading(reading, &mut vec![], &mut settled).0)
            .collect()
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
                ConditionGoal::Chosen { pattern, name } => {
                    let (resolved, stands_alone) = self.resolve_goal(&name, in_progress, settled);
                    self_contained &= stands_alone;
                    let matched = pattern.spans_on(&name).unwrap_or_default();
                    let mut chosen =
                        Resolution::new(pattern.source()).with_bound(pattern.bound().to_vec());
                    for offer in resolved.get_satisfying_offers() {
                        chosen = chosen.add_satisfying_offer(
                            offer.clone().answering(&name).with_matched(matched.clone()),
                        );
                    }
                    for offer in resolved.get_unsatisfying_offers() {
                        chosen = chosen.add_unsatisfying_offer(
                            offer.clone().answering(&name).with_matched(matched.clone()),
                        );
                    }
                    resolved_conditions.push(chosen);
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

    /// The ways a promise's conditions can be read together.
    ///
    /// `conditions` are the promise's, with whatever the goal bound already
    /// filled in. Usually nothing more is needed and there is one reading:
    /// each condition is a goal of its own. But a condition may open a
    /// variable, `will support {{?thing}}`, that a sibling uses plainly,
    /// `{{thing}} does stuff`. The two then have to agree on the value, so
    /// every behavior the open condition matches is one candidate, and each
    /// candidate is a reading of its own: the open condition answered by that
    /// match alone, its siblings with the value filled in.
    ///
    /// When the open condition matches nothing there is still one reading, so
    /// that the promise shows as an offer that cannot be kept, with the
    /// waiting conditions left as written.
    fn condition_readings(&self, conditions: &[Pattern]) -> Vec<Vec<ConditionGoal>> {
        fn plain<'a>(p: &'a Pattern) -> impl Iterator<Item = &'a str> {
            p.vars().into_iter().filter(|v| !components::is_open(v))
        }
        let shared: BTreeSet<String> = conditions
            .iter()
            .flat_map(|c| plain(c).map(str::to_string))
            .collect();
        if shared.is_empty() {
            return vec![conditions
                .iter()
                .map(|c| ConditionGoal::Goal(c.clone()))
                .collect()];
        }
        let as_written = |c: &Pattern| {
            if c.needs_binding() {
                ConditionGoal::Unbound(c.clone())
            } else {
                ConditionGoal::Goal(c.clone())
            }
        };

        // The conditions that find a shared value, each taken once whatever
        // it waits on itself has been found.
        let mut pending: Vec<usize> = (0..conditions.len())
            .filter(|i| {
                conditions[*i]
                    .vars()
                    .into_iter()
                    .any(|v| components::is_open(v) && shared.contains(&v[1..]))
            })
            .collect();
        let mut known: BTreeSet<String> = BTreeSet::new();
        // One entry per reading so far: the values found, and which match
        // each finding condition was answered by.
        let mut readings: Vec<(components::Bindings, BTreeMap<usize, (Pattern, String)>)> =
            vec![(components::Bindings::new(), BTreeMap::new())];
        while let Some(at) = pending
            .iter()
            .position(|i| plain(&conditions[*i]).all(|v| known.contains(v)))
        {
            let i = pending.remove(at);
            let mut next = vec![];
            for (bindings, chosen) in &readings {
                let pattern = conditions[i].substitute(bindings);
                for name in self.open_matches(pattern.source()).unwrap_or_default() {
                    let Some(found) = pattern.match_ground(&name) else {
                        continue;
                    };
                    let mut bindings = bindings.clone();
                    let mut agrees = true;
                    for (variable, value) in found {
                        let key = &variable[1..];
                        if !components::is_open(&variable) || !shared.contains(key) {
                            continue;
                        }
                        match bindings.get(key) {
                            Some(held) if held != &value => agrees = false,
                            _ => {
                                bindings.insert(key.to_string(), value);
                            }
                        }
                    }
                    if agrees && next.len() < MAX_CONDITION_READINGS {
                        let mut chosen = chosen.clone();
                        chosen.insert(i, (pattern.clone(), name));
                        next.push((bindings, chosen));
                    }
                }
            }
            if next.is_empty() {
                return vec![conditions.iter().map(as_written).collect()];
            }
            readings = next;
            known.extend(
                conditions[i]
                    .vars()
                    .into_iter()
                    .filter(|v| components::is_open(v))
                    .map(|v| v[1..].to_string()),
            );
        }

        readings
            .into_iter()
            .map(|(bindings, chosen)| {
                conditions
                    .iter()
                    .enumerate()
                    .map(|(i, c)| match chosen.get(&i) {
                        Some((pattern, name)) => ConditionGoal::Chosen {
                            pattern: pattern.clone(),
                            name: name.clone(),
                        },
                        None => as_written(&c.substitute(&bindings)),
                    })
                    .collect()
            })
            .collect()
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

        // Its conditions: the open one, with the goal's value; and its
        // sibling, with the goal's value and the one the open one found.
        let conditions = awssm.get_resolved_conditions();
        assert_eq!(
            conditions[0].marked_name(),
            "<appname> has IAM role {{?approle}}"
        );
        // The behavior kept under the open condition: what the goal filled
        // in, what `{{?approle}}` matched, and the provider's own name.
        let role = &conditions[0].get_satisfying_offers()[0];
        assert_eq!(role.get_behavior(), Some("appname has IAM role approle"));
        assert_eq!(
            conditions[0].marked_offer(role),
            "<appname> has IAM role <approle>"
        );
        assert_eq!(
            conditions[1].marked_name(),
            "IAM access granted for reading Secret <appsecret> to <approle>"
        );

        // One level down, infrateam's open condition and manager's promise.
        let grant = &conditions[1].get_satisfying_offers()[0];
        let approval = &grant.get_resolved_conditions()[0];
        assert_eq!(
            approval.marked_name(),
            "Design approval access to Secret <appsecret> for {{?application}}"
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
                "    <appname> has IAM role {{?approle}}",
                "      appname: <appname> has IAM role <approle>",
                "    IAM access granted for reading Secret <appsecret> to <approle>",
                "      infrateam: IAM access granted for reading Secret <appsecret> to <approle>",
                "        Design approval access to Secret <appsecret> for {{?application}}",
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
        let mut t = Tracker::new();
        t.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("{{?who}} backs up {{!self}}")]));
        t.add_agent(Agent::build("b1").with_provides(vec![Behavior::build("{{!self}} backs up a1")]));

        // By text: only the open match and the provider's own name.
        let plain = t.resolve("{{?who}} backs up a1");
        assert_eq!(plain.marked_name(), "{{?who}} backs up a1");
        assert_eq!(
            plain.marked_offer(&plain.get_satisfying_offers()[0]),
            "<b1> backs up a1"
        );
        // As a1 wants it: its own name too.
        let wanted = t.resolve_want("a1", "{{?who}} backs up a1");
        assert_eq!(wanted.marked_name(), "{{?who}} backs up <a1>");
        assert_eq!(
            wanted.marked_offer(&wanted.get_satisfying_offers()[0]),
            "<b1> backs up <a1>"
        );
        assert_eq!(
            plain.marked_outline(),
            vec!["{{?who}} backs up a1", "  b1: <b1> backs up a1"]
        );
        assert_eq!(
            wanted.marked_outline(),
            vec!["{{?who}} backs up <a1>", "  b1: <b1> backs up <a1>"]
        );
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
        .with_conditions(vec![String::from("{{?doc}} shows usage of R1 by {{agent}}")])]));
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

    #[test]
    fn test_open_condition_is_met_by_whoever_provides_a_match() {
        let t = open_chain(&[("D1", true)]);
        assert!(t.resolve("A1 reads R1").is_satisfied());
        assert_eq!(
            t.resolve("{{?doc}} shows usage of R1 by A1"),
            Resolution::new("{{?doc}} shows usage of R1 by A1").add_satisfying_offer(
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
        let r = t.resolve("{{?doc}} shows usage of R1 by A1");
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
        assert_eq!(
            t.resolve("{{?doc}} shows usage of R1 by A1"),
            Resolution::new("{{?doc}} shows usage of R1 by A1")
        );
        assert!(!t.resolve("A1 reads R1").is_satisfied());
        assert!(!t.has_provider("{{?doc}} shows usage of R1 by A1"));
    }

    #[test]
    fn test_open_want_is_legitimate_and_skips_parameterized_providers() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("A1").with_wants(vec![
            Behavior::build("{{?svc}} grants {{!self}} read on R1"),
            Behavior::build("{{?who}} backs up R1"),
        ]));
        // a pattern on the provider's side: not matched by an open goal
        t.add_agent(
            Agent::build("S1")
                .with_provides(vec![Behavior::build("{{!self}} grants {{agent}} read on R1")]),
        );
        t.add_agent(Agent::build("B1").with_provides(vec![Behavior::build("{{!self}} backs up R1")]));

        // an open want is not a gap in the contract
        assert!(t.non_ground_wants().is_empty());
        assert!(!t.resolve("{{?svc}} grants A1 read on R1").is_satisfied());
        assert_eq!(
            t.resolve("{{?who}} backs up R1"),
            Resolution::new("{{?who}} backs up R1")
                .add_satisfying_offer(Offer::new("B1").answering("B1 backs up R1"))
        );
        // while a want with a plain variable still is one
        t.add_agent(Agent::build("W").with_wants(vec![Behavior::build("{{?a}} and {{b}}")]));
        assert_eq!(
            t.non_ground_wants(),
            vec![("W".to_string(), "{{?a}} and {{b}}".to_string())]
        );
    }

    /// O1 keeps `configured` when something it will support also does
    /// stuff: the two conditions have to be about the same thing.
    fn shared_value(does_stuff: &[&str]) -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_conditions(vec![
                String::from("will support {{?thing}}"),
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
        Resolution::new("will support {{?thing}}").add_satisfying_offer(
            Offer::new("S1").answering(&format!("will support {}", thing)),
        )
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
        // T3 does stuff, but nothing will support it
        let t = shared_value(&["T3"]);
        let r = t.resolve("configured");
        assert!(!r.is_satisfied());
        assert_eq!(r.get_unsatisfying_offers().len(), 2);
    }

    #[test]
    fn test_shared_value_nothing_found_leaves_the_rest_as_written() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_conditions(vec![
                String::from("{{thing}} does stuff"),
                String::from("will support {{?thing}}"),
            ])]));
        assert_eq!(
            t.resolve("configured"),
            Resolution::new("configured").add_unsatisfying_offer(Offer::new_conditional(
                "O1",
                vec![
                    Resolution::new("{{thing}} does stuff"),
                    Resolution::new("will support {{?thing}}")
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
        .with_conditions(vec![
            String::from("{{replica}} is healthy"),
            String::from("{{?replica}} runs in {{region}}"),
            String::from("{{?region}} hosts {{team}}"),
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
        assert_eq!(conditions[1].get_behavior_name(), "{{?replica}} runs in east");
        assert!(!t.resolve("placed for green").is_satisfied());
    }

    #[test]
    fn test_shared_value_on_a_loop_is_reported() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("O1").with_provides(vec![Behavior::build("configured")
            .with_conditions(vec![
                String::from("will support {{?thing}}"),
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
        t.add_agent(
            Agent::build("a")
                .with_provides(vec![Behavior::build("a is ready")
                    .with_conditions(vec![String::from("{{?x}} is ready")])]),
        );
        assert!(!t.resolve("a is ready").is_satisfied());
        assert_eq!(
            t.dependency_cycles(),
            vec![vec!["a is ready".to_string(), "{{?x}} is ready".to_string()]]
        );
    }

    #[test]
    fn test_open_condition_passes_through_a_folded_collective() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("op").with_provides(vec![
            Behavior::build("configured").with_conditions(vec![String::from("{{?doc}} shows usage")]),
        ]));
        t.add_agent(Agent::build("d1").with_provides(vec![Behavior::build("{{!self}} shows usage")]));
        t.add_superagent(
            SuperAgent::new(String::from("team"))
                .with_agent("op")
                .with_agent("d1"),
        );
        assert_eq!(t.get_working_agent_names(), vec!["team"]);
        // the open condition is not reduced away; the collective meets it itself
        assert_eq!(
            t.resolve("configured"),
            Resolution::new("configured").add_satisfying_offer(Offer::new_conditional(
                "team",
                vec![Resolution::new("{{?doc}} shows usage")
                    .add_satisfying_offer(Offer::new("team").answering("d1 shows usage"))]
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

    #[test]
    fn test_unneeded_alternatives_fold_among_open_variable_candidates() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w").with_wants(vec![Behavior::build("{{?who}} is up")]));
        t.add_agent(Agent::build("a").with_provides(vec![Behavior::build("a is up")]));
        t.add_agent(Agent::build("b").with_provides(vec![conditional("b is up", &["power"])]));

        let full = t.resolve("{{?who}} is up");
        assert_eq!(
            full.to_strings_compressed(false),
            vec![
                "{{?who}} is up |-> a",
                "               |-> b &-> power |-> ?",
            ]
        );
        let pruned = full.without_unneeded_alternatives();
        assert_eq!(
            pruned,
            Resolution::new("{{?who}} is up")
                .add_satisfying_offer(Offer::new("a").answering("a is up"))
        );
        assert_eq!(
            pruned.to_strings_compressed(false),
            vec!["{{?who}} is up |-> a"]
        );
    }

    #[test]
    fn test_unneeded_alternatives_keep_every_candidate_when_none_holds() {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("w").with_wants(vec![Behavior::build("{{?who}} is up")]));
        t.add_agent(Agent::build("a").with_provides(vec![conditional("a is up", &["fuel"])]));
        t.add_agent(Agent::build("b").with_provides(vec![conditional("b is up", &["power"])]));

        let full = t.resolve("{{?who}} is up");
        let pruned = full.clone().without_unneeded_alternatives();
        assert_eq!(pruned, full);
        assert_eq!(
            pruned.to_strings_compressed(false),
            full.to_strings_compressed(false)
        );
        assert_eq!(
            pruned.to_strings_compressed(false),
            vec![
                "{{?who}} is up |-> a &-> fuel |-> ?",
                "               |-> b &-> power |-> ?",
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
