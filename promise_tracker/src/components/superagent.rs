use serde::{Deserialize,Serialize};
use schemars::JsonSchema;
use crate::components::behavior::Behavior;

#[derive(Debug,PartialEq,Clone)]
#[derive(Deserialize,Serialize)]
#[derive(JsonSchema)]
struct SuperAgentInstance {
  name: String,
  provides_tag: String,
  conditions_tag: String,

  #[serde(default)]
  provides: Vec<Behavior>,

  #[serde(default)]
  wants: Vec<Behavior>,
}

#[derive(Debug,PartialEq)]
#[derive(Deserialize,Serialize)]
#[derive(JsonSchema)]
pub struct SuperAgent {
  name: String,

  #[serde(default)]
  agents: Vec<String>,

  #[serde(default)]
  instances: Vec<SuperAgentInstance>,
}

impl SuperAgent {
  fn new(name: String) -> SuperAgent {
    SuperAgent{
      name: name,
      agents: vec!(),
      instances: vec!(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_yaml::{self};
  use serde_json::json;

  #[test]
  fn simple_from_yaml() {
    let s: SuperAgent = serde_yaml::from_str("name: sa").expect("Unable to parse");
    assert_eq!(s.name, "sa");
    let ra: Vec<String> = vec!();
    assert_eq!(s.agents, ra);
    assert_eq!(s.instances, vec!());
  }

  fn deep_from_yaml() {
    let j = serde_json::to_string(&json!({
      "name": "j",
      "agents": ["a1", "a2"],
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
    })).expect("setup fail");
    let s: SuperAgent = serde_yaml::from_str(&j).expect("Unable to parse");
    assert_eq!(s.name, "j");
    assert_eq!(s.agents, ["a1", "a2"]);
    assert_eq!(s.instances[0].name, "i1");
    assert_eq!(s.instances[0].provides[0].get_name(), "p1");
    assert_eq!(s.instances[0].provides[1].get_conditions(), ["c1", "c2"]);
  }

}