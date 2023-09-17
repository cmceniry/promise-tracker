use schemars::{schema_for,JsonSchema};
use serde;
use serde_json::{self};

use promise_tracker::components::SuperAgent;
use promise_tracker::components::Agent;

#[derive(JsonSchema)]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
enum PTComponent {
    Agent(Agent),
    SuperAgent(SuperAgent),
}

fn main() {
    let schema = schema_for!(PTComponent);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
