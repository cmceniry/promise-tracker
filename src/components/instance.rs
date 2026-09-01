//! A copy of a component, with values filled in.
//!
//! An [`Instance`] names one real agent built from a base — an `Agent` or a
//! `SuperAgent` — with its `bindings` substituted through everything the base
//! promises and needs. Where a parameterized behavior lets a consumer supply
//! the value, an instance supplies it itself, which is what makes two copies of
//! one definition tell apart.
//!
//! See `docs/design/parameterized-behaviors.md`.

use crate::components::agent::Agent;
use crate::components::behavior::Behavior;
use crate::components::pattern::Bindings;
use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseKind {
    Agent,
    SuperAgent,
}

impl fmt::Display for BaseKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BaseKind::Agent => f.write_str("Agent"),
            BaseKind::SuperAgent => f.write_str("SuperAgent"),
        }
    }
}

/// What an instance is built from, written as `Agent/name`,
/// `SuperAgent/name`, or a bare `name` when only one thing goes by it.
///
/// The qualified form is the same `Kind/name` that `Item::get_name` produces,
/// so the two spellings of a component's identity agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseRef {
    kind: Option<BaseKind>,
    name: String,
}

impl BaseRef {
    pub fn parse(source: &str) -> BaseRef {
        match source.split_once('/') {
            Some(("Agent", name)) => BaseRef {
                kind: Some(BaseKind::Agent),
                name: name.to_string(),
            },
            Some(("SuperAgent", name)) => BaseRef {
                kind: Some(BaseKind::SuperAgent),
                name: name.to_string(),
            },
            // Anything else is a bare name. A component may legitimately have
            // a `/` in it, and only these two prefixes mean a kind.
            _ => BaseRef {
                kind: None,
                name: source.to_string(),
            },
        }
    }

    pub fn kind(&self) -> Option<BaseKind> {
        self.kind
    }

    pub fn name(&self) -> &String {
        &self.name
    }
}

impl fmt::Display for BaseRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            Some(kind) => write!(f, "{}/{}", kind, self.name),
            None => f.write_str(&self.name),
        }
    }
}

impl Serialize for BaseRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BaseRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Ok(BaseRef::parse(&source))
    }
}

impl JsonSchema for BaseRef {
    fn schema_name() -> String {
        String::schema_name()
    }
    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        String::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    name: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    comment: String,

    base: BaseRef,

    /// Values for the base's variables. What they do not cover stays
    /// parameterized, for a consumer to supply.
    #[serde(default)]
    #[serde(skip_serializing_if = "Bindings::is_empty")]
    bindings: Bindings,

    /// Promises this copy makes and no other does.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    provides: Vec<Behavior>,

    /// Needs this copy has and no other does.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    wants: Vec<Behavior>,
}

impl Instance {
    pub fn new(name: &str, base: &str) -> Instance {
        Instance {
            name: name.to_string(),
            comment: String::new(),
            base: BaseRef::parse(base),
            bindings: Bindings::new(),
            provides: vec![],
            wants: vec![],
        }
    }

    pub fn with_binding(mut self, variable: &str, value: &str) -> Instance {
        self.bindings
            .insert(variable.to_string(), value.to_string());
        self
    }

    pub fn with_provides(mut self, provides: Vec<Behavior>) -> Instance {
        self.provides = provides;
        self
    }

    pub fn with_wants(mut self, wants: Vec<Behavior>) -> Instance {
        self.wants = wants;
        self
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_base(&self) -> &BaseRef {
        &self.base
    }

    pub fn get_bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub fn get_provides(&self) -> &[Behavior] {
        &self.provides
    }

    pub fn get_wants(&self) -> &[Behavior] {
        &self.wants
    }

    /// The working agent this instance stands for: the base's promises and
    /// needs with `bindings` substituted through, plus its own.
    pub fn materialize(&self, base: &Agent) -> Agent {
        let mut agent = base.make_instance(&self.name, &self.bindings);
        for p in &self.provides {
            agent.add_provide(p.clone());
        }
        for w in &self.wants {
            agent.add_want(w.clone());
        }
        agent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::{self};

    #[test]
    fn base_may_be_qualified_or_bare() {
        let b = BaseRef::parse("SuperAgent/kube-cluster");
        assert_eq!(b.kind(), Some(BaseKind::SuperAgent));
        assert_eq!(b.name(), "kube-cluster");
        assert_eq!(b.to_string(), "SuperAgent/kube-cluster");

        let b = BaseRef::parse("Agent/host");
        assert_eq!(b.kind(), Some(BaseKind::Agent));
        assert_eq!(b.name(), "host");

        let b = BaseRef::parse("host");
        assert_eq!(b.kind(), None);
        assert_eq!(b.name(), "host");
        assert_eq!(b.to_string(), "host");
    }

    #[test]
    fn an_unknown_prefix_is_part_of_the_name() {
        let b = BaseRef::parse("some/thing");
        assert_eq!(b.kind(), None);
        assert_eq!(b.name(), "some/thing");
    }

    #[test]
    fn from_yaml() {
        let i: Instance = serde_yaml::from_str(
            "name: prod-cluster
comment: production
base: SuperAgent/kube-cluster
bindings:
  env: prod
provides:
  - name: audit-hook
wants:
  - name: pagerduty
",
        )
        .expect("Unable to parse");
        assert_eq!(i.get_name(), "prod-cluster");
        assert_eq!(i.get_base().name(), "kube-cluster");
        assert_eq!(i.get_bindings()["env"], "prod");
        assert_eq!(i.get_provides(), &[Behavior::build("audit-hook")]);
        assert_eq!(i.get_wants(), &[Behavior::build("pagerduty")]);
    }

    #[test]
    fn a_bare_instance_needs_only_a_name_and_a_base() {
        let i: Instance = serde_yaml::from_str("name: i1\nbase: sa1\n").expect("Unable to parse");
        assert_eq!(i.get_name(), "i1");
        assert_eq!(i.get_base().name(), "sa1");
        assert!(i.get_bindings().is_empty());
        assert!(i.get_provides().is_empty());
    }

    #[test]
    fn to_yaml_keeps_the_qualified_base() {
        let i = Instance::new("i1", "SuperAgent/sa1").with_binding("env", "prod");
        let out = serde_yaml::to_string(&i).expect("Unable to serialize");
        assert_eq!(
            out,
            "name: i1\nbase: SuperAgent/sa1\nbindings:\n  env: prod\n"
        );
    }

    #[test]
    fn materialize_substitutes_through_the_base() {
        let base = Agent::build("kube-api")
            .with_provides(vec![Behavior::build("kube-api/{{env}}")
                .with_conditions(vec![String::from("etcd/{{env}}")])]);
        let instance = Instance::new("prod", "Agent/kube-api")
            .with_binding("env", "prod")
            .with_provides(vec![Behavior::build("audit")]);

        let agent = instance.materialize(&base);
        assert_eq!(agent.get_name(), "prod");
        let mut provides = agent.provides().to_vec();
        provides.sort();
        assert_eq!(
            provides,
            vec![
                Behavior::build("audit"),
                Behavior::build("kube-api/prod").with_conditions(vec![String::from("etcd/prod")]),
            ]
        );
    }

    #[test]
    fn materialize_leaves_what_it_cannot_fill_in() {
        let base = Agent::build("kube-api")
            .with_provides(vec![Behavior::build("kube-api/{{env}}/{{tenant}}")]);
        let agent = Instance::new("prod", "kube-api")
            .with_binding("env", "prod")
            .materialize(&base);
        // `tenant` is still open, for whoever wants it to name.
        assert_eq!(
            agent.provides(),
            &[Behavior::build("kube-api/prod/{{tenant}}")]
        );
    }
}
