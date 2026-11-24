use crate::components::pattern::{Bindings, Pattern};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// A promise: a name, and the conditions its keeper needs met to keep it.
//
// The name and each condition are `Pattern`s, so a provider can describe a
// family of promises at once. `get_name` and `get_conditions` keep returning
// the source text, so everything that treated a behavior name as a string
// still does; the pattern accessors are for code that needs to match or
// substitute.
//
// `forSome` declares variables the conditions find values for, rather than
// taking them from the promise name (`docs/design/self-and-open-variables.md`
// §3). It is written before the conditions it serves.
//
// Deliberately not a doc comment: schemars copies those into the generated
// JSON schema, and this type's schema is meant to stay byte-identical.
#[derive(
    Debug, PartialEq, Eq, Deserialize, Serialize, Clone, Hash, JsonSchema, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub struct Behavior {
    name: Pattern,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    comment: String,

    #[serde(default)]
    #[serde(rename = "forSome")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    for_some: Vec<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conditions: Vec<Pattern>,
}

impl Behavior {
    pub fn new(name: String) -> Behavior {
        Behavior {
            name: Pattern::parse_lossy(&name),
            comment: String::from(""),
            for_some: vec![],
            conditions: vec![],
        }
    }

    pub fn new_with_conditions(name: String, conditions: Vec<String>) -> Behavior {
        Behavior {
            name: Pattern::parse_lossy(&name),
            comment: String::from(""),
            for_some: vec![],
            conditions: conditions.iter().map(|c| Pattern::parse_lossy(c)).collect(),
        }
    }

    pub fn build(name: &str) -> Behavior {
        Behavior::new(String::from(name))
    }

    pub fn with_conditions(mut self, conditions: Vec<String>) -> Behavior {
        self.conditions = conditions.iter().map(|c| Pattern::parse_lossy(c)).collect();
        self
    }

    pub fn with_condition_patterns(mut self, conditions: Vec<Pattern>) -> Behavior {
        self.conditions = conditions;
        self
    }

    /// Declare the variables this promise's conditions find values for.
    pub fn with_for_some(mut self, variables: Vec<String>) -> Behavior {
        self.for_some = variables;
        self
    }

    pub fn add_condition(&mut self, c: String) {
        let c = Pattern::parse_lossy(&c);
        if self.conditions.contains(&c) {
            return;
        }
        if c.source().is_empty() {
            return;
        }
        if c == self.name {
            return;
        }
        self.conditions.push(c)
    }

    /// The name as written.
    pub fn get_name(&self) -> &String {
        self.name.source()
    }

    pub fn get_name_pattern(&self) -> &Pattern {
        &self.name
    }

    /// The comment written beside the promise, empty when there is none.
    pub fn get_comment(&self) -> &str {
        &self.comment
    }

    /// The variables declared with `forSome`, as written.
    pub fn get_for_some(&self) -> &[String] {
        &self.for_some
    }

    /// The conditions as written.
    pub fn get_conditions(&self) -> Vec<String> {
        self.conditions.iter().map(|c| c.source().clone()).collect()
    }

    pub fn get_condition_patterns(&self) -> &[Pattern] {
        &self.conditions
    }

    pub fn is_unconditional(&self) -> bool {
        self.conditions.len() == 0
    }

    /// This promise names exactly one behavior, and so do all its conditions.
    pub fn is_ground(&self) -> bool {
        self.name.is_ground() && self.conditions.iter().all(|c| c.is_ground())
    }

    /// Bind this promise's variables from a concrete goal, if it can keep it.
    pub fn match_goal(&self, goal: &str) -> Option<Bindings> {
        self.name.match_ground(goal)
    }

    /// A copy with `bindings` substituted through the name and the conditions.
    ///
    /// A variable the promise declares with `forSome` is its own, found by
    /// its conditions, so a binding of the same name does not reach it.
    pub fn instantiate(&self, bindings: &Bindings) -> Behavior {
        let own;
        let bindings = if self.for_some.iter().any(|v| bindings.contains_key(v)) {
            let mut kept = bindings.clone();
            kept.retain(|variable, _| !self.for_some.contains(variable));
            own = kept;
            &own
        } else {
            bindings
        };
        Behavior {
            name: self.name.substitute(bindings),
            comment: self.comment.clone(),
            for_some: self.for_some.clone(),
            conditions: self
                .conditions
                .iter()
                .map(|c| c.substitute(bindings))
                .collect(),
        }
    }

    /// A copy with `{{!self}}` replaced by the name of the agent that wrote
    /// it, through the name and the conditions.
    pub fn bind_self(&self, name: &str) -> Behavior {
        self.instantiate(&crate::components::pattern::self_bindings(name))
    }

    pub fn has_none_of_these_conditions(&self, conditions: &HashSet<String>) -> bool {
        !self
            .conditions
            .iter()
            .any(|c| conditions.contains(c.source()))
    }

    pub fn has_behavior(&self, behavior_name: &String) -> bool {
        self.get_name() == behavior_name
            || self.conditions.iter().any(|x| x.source() == behavior_name)
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
    fn for_some_reads_and_writes_before_the_conditions() {
        let yaml = "name: '{{app}} gateway is configured'\nforSome:\n- kc\nconditions:\n- '{{app}} deploys to {{kc}}'\n- '{{kc}} deploys {{app}} gateway'\n";
        let p: Behavior = serde_yaml::from_str(yaml).expect("Unable to parse");
        assert_eq!(p.get_for_some(), &["kc".to_string()]);
        assert_eq!(serde_yaml::to_string(&p).unwrap(), yaml);
        // and a promise without it writes nothing for it
        let plain: Behavior = serde_yaml::from_str("name: foo").unwrap();
        assert!(plain.get_for_some().is_empty());
        assert_eq!(serde_yaml::to_string(&plain).unwrap(), "name: foo\n");
    }

    #[test]
    fn instantiate_leaves_declared_variables_alone() {
        let p = Behavior::build("{{app}} is up")
            .with_for_some(vec!["kc".into()])
            .with_conditions(vec!["{{app}} runs on {{kc}}".into()]);
        let bindings = Bindings::from([
            ("app".to_string(), "web".to_string()),
            ("kc".to_string(), "c1".to_string()),
        ]);
        let out = p.instantiate(&bindings);
        assert_eq!(out.get_name(), "web is up");
        assert_eq!(out.get_conditions(), vec!["web runs on {{kc}}".to_string()]);
        assert_eq!(out.get_for_some(), &["kc".to_string()]);
    }

    #[test]
    fn add_condition() {
        let mut p = Behavior {
            name: Pattern::from("a"),
            comment: String::from(""),
            for_some: vec![],
            conditions: vec![],
        };
        p.add_condition(String::from("c1"));
        assert!(p.conditions == ["c1"]);
        p.add_condition(String::from("c2"));
        assert!(p.conditions == ["c1", "c2"]);
        // test duplicate
        p.add_condition(String::from("c1"));
        assert!(p.conditions == ["c1", "c2"]);
        // test empty
        p.add_condition(String::from(""));
        assert!(p.conditions == ["c1", "c2"]);
        // test self-reference
        p.add_condition(String::from("a"));
        assert!(p.conditions == ["c1", "c2"]);
    }

    #[test]
    fn is_conditional() {
        let mut p = Behavior {
            name: Pattern::from("a"),
            comment: String::from(""),
            for_some: vec![],
            conditions: vec![],
        };
        assert!(p.is_unconditional());
        p.add_condition(String::from("c1"));
        assert!(!p.is_unconditional());
    }

    #[test]
    fn test_has_none_of_these_conditions() {
        let p = Behavior {
            name: Pattern::from("b1"),
            comment: String::from(""),
            for_some: vec![],
            conditions: vec![Pattern::from("c1"), Pattern::from("c2")],
        };
        let mut conditions = HashSet::new();
        assert!(p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c99"));
        assert!(p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c1"));
        assert!(!p.has_none_of_these_conditions(&conditions));
        conditions.insert(String::from("c2"));
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
