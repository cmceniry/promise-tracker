use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use crate::components::behavior::Behavior;

#[derive(Debug,PartialEq,Eq,Clone)]
#[derive(Deserialize,Serialize)]
#[derive(JsonSchema)]
pub struct Agent {
  name: String,

  #[serde(default)]
  provides: Vec<Behavior>,

  #[serde(default)]
  wants: Vec<Behavior>,
}

impl Agent {
  pub fn new(name: String) -> Agent {
    Agent{
      name: name,
      provides: vec!(),
      wants: vec!(),
    }
  }

  pub fn get_name(&self) -> &String {
    &self.name
  }

  pub fn add_provide(&mut self, p: Behavior) {
    self.provides.push(p)
  }

  pub fn add_want(&mut self, w: Behavior) {
    self.provides.push(w)
  }

}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_yaml::{self};

  #[test]
  fn simple() {
    let a = Agent::new(String::from("foo"));
    assert_eq!(a.name, "foo");
    assert_eq!(a.get_name(), "foo");
  }

  #[test]
  fn simple_from_yaml() {
    let a:Agent = serde_yaml::from_str("name: foo").expect("Unable to parse");
    assert_eq!(a.name, "foo");
    assert_eq!(a.provides, vec!());
    assert_eq!(a.wants, vec!());
  }

  // #[test]
  // fn deep_from_yaml() {
  //   let a:Agent = serde_yaml
  // }

}