use wasm_bindgen::prelude::*;
use schemars::JsonSchema;
use serde_json::{self};
use promise_tracker::components::Agent;
use promise_tracker::components::SuperAgent;
use promise_tracker;

#[derive(JsonSchema)]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
enum PTComponent {
    SuperAgent(SuperAgent),
    Agent(Agent),
}

#[wasm_bindgen]
pub fn get_schema() -> String {
    let schema = schemars::schema_for!(PTComponent);
    serde_json::to_string_pretty(&schema).unwrap()
}

#[wasm_bindgen]
pub struct PT {
    tracker: promise_tracker::Tracker,
}

#[wasm_bindgen]
pub fn get_pt() -> PT {
    PT {
        tracker: promise_tracker::Tracker::new(),
    }
}

#[wasm_bindgen]
impl PT {
    pub fn add_stuff(& mut self, input: &str) {
        self.tracker.add_agent(Agent::build(input));
    }

    pub fn check(&self, input: &str) -> bool {
        self.tracker.has_agent(String::from(input))
    }
}
