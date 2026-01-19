use crate::components::behavior::Behavior;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SuperAgentInstance {
    name: String,
    #[serde(default)]
    comment: String,
    provides_tag: String,
    conditions_tag: String,

    #[serde(default)]
    provides: Vec<Behavior>,

    #[serde(default)]
    wants: Vec<Behavior>,
}

impl SuperAgentInstance {
    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_provides(&self) -> &Vec<Behavior> {
        &self.provides
    }

    pub fn get_wants(&self) -> &Vec<Behavior> {
        &self.wants
    }

    pub fn get_provides_tags(&self) -> &String {
        &self.provides_tag
    }

    pub fn get_conditions_tags(&self) -> &String {
        &self.conditions_tag
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuperAgent {
    name: String,

    #[serde(default)]
    comment: String,

    #[serde(default)]
    agents: Vec<String>,

    #[serde(default)]
    instances: Vec<SuperAgentInstance>,
}

impl SuperAgent {
    pub fn new(name: String) -> SuperAgent {
        SuperAgent {
            name: name,
            comment: String::from(""),
            agents: vec![],
            instances: vec![],
        }
    }

    pub fn with_agent(mut self, agent: &str) -> SuperAgent {
        if self.agents.contains(&String::from(agent)) {
            return self;
        }
        self.agents.push(String::from(agent));
        self
    }

    pub fn with_instance(
        mut self,
        name: &str,
        comment: &str,
        provides_tag: &str,
        conditions_tag: &str,
        provides: Vec<Behavior>,
        wants: Vec<Behavior>,
    ) -> SuperAgent {
        if self.instances.iter().any(|i| i.name == self.name) {
            return self;
        }
        self.instances.push(SuperAgentInstance {
            name: name.to_string(),
            comment: comment.to_string(),
            provides_tag: provides_tag.to_string(),
            conditions_tag: conditions_tag.to_string(),
            provides: provides,
            wants: wants,
        });
        self
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_agent_names(&self) -> Vec<String> {
        self.agents.clone()
    }

    pub fn get_instance_names(&self) -> Vec<String> {
        self.instances.iter().map(|i| i.name.clone()).collect()
    }

    pub fn get_instances(&self) -> Vec<SuperAgentInstance> {
        self.instances.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Behavior;
    use serde_json::json;
    use serde_yaml::{self};

    #[test]
    fn simple_from_yaml() {
        let s: SuperAgent = serde_yaml::from_str("name: sa").expect("Unable to parse");
        assert_eq!(s.name, "sa");
        let ra: Vec<String> = vec![];
        assert_eq!(s.agents, ra);
        assert_eq!(s.instances, vec!());
    }

    #[test]
    fn deep_from_yaml() {
        let j = serde_json::to_string(&json!({
          "name": "j",
          "agents": ["a1", "a2"],
          "comment": "this is a comment",
          "instances": [
            {
              "name": "i1",
              "comment": "this is a comment",
              "providesTag": "jp",
              "conditionsTag": "jc",
              "provides": [
                {"name": "p1"},
                {"name": "p2", "conditions": ["c1", "c2"]},
              ],
              "wants": [{"name": "w1"}],
            },
          ],
        }))
        .expect("setup fail");
        let s: SuperAgent = serde_yaml::from_str(&j).expect("Unable to parse");
        assert_eq!(s.name, "j");
        assert_eq!(s.agents, ["a1", "a2"]);
        assert_eq!(s.instances[0].name, "i1");
        assert_eq!(s.instances[0].comment, "this is a comment");
        assert_eq!(s.instances[0].provides[0].get_name(), "p1");
        assert_eq!(s.instances[0].provides[1].get_conditions(), ["c1", "c2"]);
    }

    #[test]
    fn test_simple_build() {
        let sa = SuperAgent::new("sa".to_string())
            .with_agent("a1")
            .with_agent("a2")
            .with_instance(
                "i1",
                "this is a comment",
                "jp",
                "jc",
                vec![Behavior::new("p1".to_string())],
                vec![Behavior::new("w1".to_string())],
            );
        // construction check
        assert_eq!(sa.name, "sa");
        assert_eq!(sa.agents, vec!(String::from("a1"), String::from("a2")));
        assert_eq!(sa.instances[0].name, "i1");
        assert_eq!(
            sa.instances[0].provides,
            vec!(Behavior::new("p1".to_string()))
        );
        // getters
        assert_eq!(
            sa.get_agent_names(),
            vec!(String::from("a1"), String::from("a2"))
        );
        assert_eq!(sa.get_instance_names(), vec!(String::from("i1")));
        assert_eq!(
            sa.get_instances(),
            vec![SuperAgentInstance {
                name: "i1".to_string(),
                comment: "this is a comment".to_string(),
                provides_tag: "jp".to_string(),
                conditions_tag: "jc".to_string(),
                provides: vec![Behavior::build("p1")],
                wants: vec![Behavior::build("w1")],
            }]
        );
    }

    #[test]
    fn test_superagentinstance() {
        let sai = SuperAgentInstance {
            name: "i1".to_string(),
            comment: "this is a comment".to_string(),
            provides_tag: "jp".to_string(),
            conditions_tag: "jc".to_string(),
            provides: vec![Behavior::build("p1")],
            wants: vec![Behavior::build("w1")],
        };
        assert_eq!(sai.get_name(), "i1");
        assert_eq!(sai.get_provides_tags(), "jp");
        assert_eq!(sai.get_conditions_tags(), "jc");
        assert_eq!(sai.get_provides(), &vec!(Behavior::build("p1")));
        assert_eq!(sai.get_wants(), &vec!(Behavior::build("w1")));
    }
}
