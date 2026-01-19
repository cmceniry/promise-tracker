use promise_tracker;
use promise_tracker::components::Agent;
use promise_tracker::components::Item;
use promise_tracker::components::SuperAgent;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{self};
use wasm_bindgen::prelude::*;

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
    tracker: Box<promise_tracker::Tracker>,
}

#[wasm_bindgen]
pub fn get_pt() -> PT {
    console_error_panic_hook::set_once();
    PT {
        tracker: Box::new(promise_tracker::Tracker::new()),
    }
}

#[wasm_bindgen]
impl PT {
    pub fn add_contract(&mut self, input: &str) {
        for document in serde_yaml::Deserializer::from_str(input) {
            let i = Item::deserialize(document).unwrap();
            self.tracker.add_item(i);
        }
    }

    pub fn check(&self, input: &str) -> bool {
        self.tracker.has_agent(String::from(input))
    }

    pub fn get_agents(&self) -> String {
        serde_json::to_string_pretty(&self.tracker.get_working_agent_names()).unwrap()
    }

    pub fn get_agent_names(&self) -> Vec<String> {
        self.tracker
            .get_working_agent_names()
            .into_iter()
            .map(|x| x.clone())
            .collect()
    }

    pub fn has_agent(&self, input: &str) -> bool {
        self.tracker.has_agent(String::from(input))
    }

    pub fn get_agent_wants(&self, input: &str) -> Vec<String> {
        let mut w: Vec<String> = self
            .tracker
            .get_agent_wants(String::from(input))
            .into_iter()
            .map(|x| x)
            .collect();
        w.sort();
        w
    }

    pub fn has_agent_want(&self, agent: &str, want: &str) -> bool {
        self.tracker
            .get_agent_wants(String::from(agent))
            .contains(&String::from(want))
    }

    pub fn is_empty(&self) -> bool {
        self.tracker.is_empty()
    }

    pub fn has_behavior(&self, input: &str) -> bool {
        self.tracker.has_behavior(String::from(input))
    }

    pub fn resolve(&mut self, input: &str) -> JsValue {
        let r = self.tracker.resolve(input);
        serde_wasm_bindgen::to_value(&r).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_contract() {
        let mut pt = get_pt();
        pt.add_contract(
            "kind: Agent
name: foo
---
kind: Agent
name: bar
",
        );
        assert_eq!(pt.check("foo"), true);
        assert_eq!(pt.check("bar"), true);
    }
}
