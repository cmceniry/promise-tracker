use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A collection of Agents, flattened into one working agent.
///
/// Copies of a collective are `kind: Instance` documents naming it as their
/// base; a collective that something instantiates is a template and does not
/// stand as a working agent itself.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuperAgent {
    name: String,

    #[serde(default)]
    comment: String,

    #[serde(default)]
    agents: Vec<String>,
}

impl SuperAgent {
    pub fn new(name: String) -> SuperAgent {
        SuperAgent {
            name: name,
            comment: String::from(""),
            agents: vec![],
        }
    }

    pub fn with_agent(mut self, agent: &str) -> SuperAgent {
        if self.agents.contains(&String::from(agent)) {
            return self;
        }
        self.agents.push(String::from(agent));
        self
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_agent_names(&self) -> Vec<String> {
        self.agents.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_yaml::{self};

    #[test]
    fn simple_from_yaml() {
        let s: SuperAgent = serde_yaml::from_str("name: sa").expect("Unable to parse");
        assert_eq!(s.name, "sa");
        let ra: Vec<String> = vec![];
        assert_eq!(s.agents, ra);
    }

    #[test]
    fn deep_from_yaml() {
        let j = serde_json::to_string(&json!({
          "name": "j",
          "agents": ["a1", "a2"],
          "comment": "this is a comment",
        }))
        .expect("setup fail");
        let s: SuperAgent = serde_yaml::from_str(&j).expect("Unable to parse");
        assert_eq!(s.name, "j");
        assert_eq!(s.agents, ["a1", "a2"]);
        assert_eq!(s.comment, "this is a comment");
    }

    #[test]
    fn instances_are_their_own_kind_now() {
        // Copies of a collective are `kind: Instance` documents naming it as
        // their base, so a collective no longer nests them.
        let e = serde_yaml::from_str::<SuperAgent>("name: sa\ninstances:\n  - name: i1\n")
            .expect_err("expected a parse failure");
        assert!(e.to_string().contains("unknown field `instances`"), "{}", e);
    }

    #[test]
    fn test_simple_build() {
        let sa = SuperAgent::new("sa".to_string())
            .with_agent("a1")
            .with_agent("a2")
            .with_agent("a1");
        assert_eq!(sa.name, "sa");
        // construction check, and a member is only listed once
        assert_eq!(sa.agents, vec!(String::from("a1"), String::from("a2")));
        // getters
        assert_eq!(sa.get_name(), "sa");
        assert_eq!(
            sa.get_agent_names(),
            vec!(String::from("a1"), String::from("a2"))
        );
    }
}
