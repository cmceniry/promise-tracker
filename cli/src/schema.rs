use schemars::schema_for;

pub fn command() {
    let schema = schema_for!(promise_tracker::components::Item);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
