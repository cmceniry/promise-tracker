use crate::components::behavior::Behavior;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SuperAgentInstance {
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuperAgent {
    name: String,

    #[serde(default)]
    agents: Vec<String>,

    #[serde(default)]
    instances: Vec<SuperAgentInstance>,
}

impl SuperAgent {
    fn new(name: String) -> SuperAgent {
        SuperAgent {
            name: name,
            agents: vec![],
            instances: vec![],
        }
    }

    pub fn get_name(&self) -> &String {
        &self.name
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
        assert_eq!(s.instances, vec!());
    }

    fn deep_from_yaml() {
        let j = serde_json::to_string(&json!({
          "name": "j",
          "agents": ["a1", "a2"],
          "comment": "this is a comment",
          "instances": [
            {
              "name": "i1",
              "provides_tag": "jp",
              "conditions_tag": "jc",
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
}
