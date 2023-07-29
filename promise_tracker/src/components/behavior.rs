use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug,PartialEq)]
#[derive(Deserialize,Serialize,Clone)]
#[derive(JsonSchema)]
pub struct Behavior {
  name: String,

  #[serde(default)]
  conditions: Vec<String>,
}

impl Behavior {
  fn new(name: String) -> Behavior {
    Behavior{
      name: name,
      conditions: vec!(),
    }
  }

  pub fn get_name(&self) -> String {
    format!("{}", self.name)
  }

  pub fn get_conditions(&self) -> Vec<String> {
    let mut ret: Vec<String> = vec!();
    for c in &self.conditions {
      ret.push(format!("{}", c));
    }
    ret
  }

  pub fn is_unconditional(&self) -> bool {
    self.conditions.len() == 0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_yaml::{self};
  // use jsonschema::JSONSchema;
  // use jsonschema::is_valid;
  // use serde_json::json;
  
  #[test]
  fn simple() {
    let p = Behavior::new(String::from("a"));
    assert!(p.name == "a");
  }

  #[test]
  fn from_yaml() {
    let p:Behavior = serde_yaml::from_str("name: foo").expect("Unable to parse");
    assert!(p.name == "foo");
    let p:Behavior = serde_yaml::from_str("name: foo\nconditions:\n  - bar\n  - baz").expect("Unable to parse");
    assert!(p.name == "foo");
    assert!(p.conditions == ["bar", "baz"]);
  }

  #[test]
  fn is_conditional() {
      let p = Behavior{name: String::from("a"), conditions: [].to_vec()};
      assert!(p.is_unconditional());
      // assert_eq!(p.conditions, .to_vec());
  }

  // #[test]
  // fn jschema() {
  //   let schema = json!({"maxLength": 5});
  //   let instance = json!("foo");
  //   let compiled = JSONSchema::compile(&schema)
  //       .expect("A valid schema");
  //   let result = compiled.validate(&instance);
  //   if let Err(errors) = result {
  //       for error in errors {
  //           println!("Validation error: {}", error);
  //           println!(
  //               "Instance path: {}", error.instance_path
  //           );
  //       }
  //   }
  //   // assert!(serde_json::from_str::<Behavior>("{}").is_ok());
  //   let schema = json!({"maxLength": 5});
  //   let instance = json!("foo");
  //   assert!(is_valid(&schema, &instance));
  //   let schema = json!({
  //     "$id": "/promise-tracker/behavior.json",
  //     "type": "object",
  //     "properties": {
  //       "name": {"$ref": "/promise-tracker/behavior-name.json"}
  //     }
  //   });
  //   assert!(JSONSchema::compile(&schema).is_ok());
  // }


}