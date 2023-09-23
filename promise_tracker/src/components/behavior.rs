use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(
    Debug, PartialEq, Eq, Deserialize, Serialize, Clone, Hash, JsonSchema, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub struct Behavior {
    name: String,
    #[serde(default)]
    comment: String,

    #[serde(default)]
    conditions: Vec<String>,
}

impl Behavior {
    pub fn new(name: String) -> Behavior {
        Behavior {
            name: name,
            comment: String::from(""),
            conditions: vec![],
        }
    }

    pub fn new_with_conditions(name: String, conditions: Vec<String>) -> Behavior {
        Behavior {
            name: name,
            comment: String::from(""),
            conditions: conditions,
        }
    }

    pub fn build(name: &str) -> Behavior {
        Behavior::new(String::from(name))
    }
    pub fn with_conditions(mut self, conditions: Vec<String>) -> Behavior {
        self.conditions = conditions;
        self
    }

    pub fn add_condition(&mut self, c: String) {
        self.conditions.push(c)
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_conditions(&self) -> Vec<String> {
        self.conditions.clone()
    }

    pub fn is_unconditional(&self) -> bool {
        self.conditions.len() == 0
    }

    pub fn has_none_of_these_conditions(&self, conditions: &HashSet<String>) -> bool {
        !self.conditions.iter().any(|c| conditions.contains(c))
    }

    pub fn has_behavior(&self, behavior_name: &String) -> bool {
        self.name == *behavior_name || self.conditions.iter().any(|x| x == behavior_name)
    }

    pub fn make_instance(&self, suffix: &str, condition_suffix: &str) -> Behavior {
        Behavior {
            name: if suffix == "" {
                self.name.clone()
            } else {
                format!("{} | {}", self.name, suffix)
            },
            comment: self.comment.clone(),
            conditions: if condition_suffix == "" {
                self.conditions.clone()
            } else {
                self.conditions
                    .iter()
                    .map(|c| format!("{} | {}", c, condition_suffix))
                    .collect()
            },
        }
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
        let p: Behavior = serde_yaml::from_str("name: foo").expect("Unable to parse");
        assert!(p.name == "foo");
        let p: Behavior = serde_yaml::from_str("name: foo\nconditions:\n  - bar\n  - baz")
            .expect("Unable to parse");
        assert!(p.name == "foo");
        assert!(p.comment == "");
        assert!(p.conditions == ["bar", "baz"]);

        assert!(p.has_behavior(&String::from("foo")));
        assert!(p.has_behavior(&String::from("bar")));
        assert!(p.has_behavior(&String::from("baz")));
        assert!(!p.has_behavior(&String::from("blah")));
    }

    #[test]
    fn is_conditional() {
        let mut p = Behavior {
            name: String::from("a"),
            comment: String::from(""),
            conditions: [].to_vec(),
        };
        assert!(p.is_unconditional());
        p.add_condition(String::from("c1"));
        assert!(!p.is_unconditional());
    }

    #[test]
    fn test_has_none_of_these_conditions() {
        let p = Behavior {
            name: String::from("b1"),
            comment: String::from(""),
            conditions: [String::from("c1"), String::from("c2")].to_vec(),
        };
        let mut conditions = HashSet::new();
        assert!(p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c99"));
        assert!(p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c1"));
        assert!(!p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c2"));
    }

    fn test_make_instance() {
        let p = Behavior {
            name: String::from("b1"),
            comment: String::from(""),
            conditions: [String::from("c1"), String::from("c2")].to_vec(),
        };
        let p2 = p.make_instance("suf", "csuf");
        assert_eq!(p2.name, "b1 | suf");
        assert_eq!(p2.conditions, ["c1 | csuf", "c2 | csuf"]);
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
