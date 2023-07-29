use schemars::schema_for;
use serde_json::{self};

use promise_tracker::components::SuperAgent;
use promise_tracker::components::Agent;

fn main() {
    let schema = schema_for!(SuperAgent);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    let schema = schema_for!(Agent);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
