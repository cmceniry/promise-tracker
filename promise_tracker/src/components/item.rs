use crate::components::agent::Agent;
use crate::components::superagent::SuperAgent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind")]
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum Item {
    Agent(Agent),
    SuperAgent(SuperAgent),
}

impl Item {
    pub fn get_name(&self) -> String {
        match self {
            Item::Agent(agent) => format!("Agent/{}", agent.get_name().clone()),
            Item::SuperAgent(superagent) => format!("SuperAgent/{}", superagent.get_name().clone()),
        }
    }
}