
#[derive(Debug)]
pub struct Resolution {
  behavior_name: String,
  satisfying_offers: Vec<Offer>,
  unsatisfying_offers: Vec<Offer>,
}

impl Resolution {
  pub fn new(behavior_name: &str) -> Resolution {
    Resolution{
      behavior_name: String::from(behavior_name),
      satisfying_offers: vec!(),
      unsatisfying_offers: vec!(),
    }
  }

  pub fn add_satisfying_offer(mut self, offer: Offer) -> Resolution {
    self.satisfying_offers.push(offer);
    self
  }

  pub fn add_satisfying_offers(mut self, offers: Vec<Offer>) -> Resolution {
    self.satisfying_offers.extend(offers);
    self
  }

  pub fn add_unsatisfying_offer(mut self, offer: Offer) -> Resolution {
    self.unsatisfying_offers.push(offer);
    self
  }

  pub fn is_satisfied(&self) -> bool {
    self.satisfying_offers.len() > 0
  }

}

impl PartialEq for Resolution {
  fn eq(&self, other: &Self) -> bool {
    if self.behavior_name != other.behavior_name {
      return false;
    }
    if self.satisfying_offers.len() != other.satisfying_offers.len() {
      return false;
    }
    for self_offer in &self.satisfying_offers {
      let mut found = false;
      for other_offer in &other.satisfying_offers {
        if self_offer == other_offer {
          found = true;
          break;
        }
      }
      if !found {
        return false;
      }
    }
    for self_offer in &self.unsatisfying_offers {
      let mut found = false;
      for other_offer in &other.unsatisfying_offers {
        if self_offer == other_offer {
          found = true;
          break;
        }
      }
      if !found {
        return false;
      }
    }
    true
  }
}
impl Eq for Resolution {}

#[cfg(test)]
mod tests_resolution {
  use super::*;

  #[test]
  fn test_eq() {
    // out of order
    assert_eq!(
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
      ,
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a2"))
        .add_satisfying_offer(Offer::new("a1"))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
    ,
    );
    // mismatch in satisfying offers count
    assert_ne!(
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
      ,
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
    ,
    );
    // mismatch in unsatisfying offers count
    assert_ne!(
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
      ,
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
    ,
    );
    // mismatch in unsatisfying offers' conditions
    assert_ne!(
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
      ,
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b3"))))
    ,
    );
    // mismatch in satisfying offers
    assert_ne!(
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a2"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
      ,
      Resolution::new("b1")
        .add_satisfying_offer(Offer::new("a1"))
        .add_satisfying_offer(Offer::new("a3"))
        .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
        .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
    ,
    );
  }
}

#[derive(Debug)]
pub struct Offer {
  agent_name: String,
  resolved_conditions: Vec<Resolution>,
}

impl Offer {
  pub fn new(agent_name: &str) -> Offer {
    Offer{
      agent_name: String::from(agent_name),
      resolved_conditions: vec!(),
    }
  }

  pub fn new_conditional(agent_name: &str, resolved_conditions: Vec<Resolution>) -> Offer {
    Offer{
      agent_name: String::from(agent_name),
      resolved_conditions,
    }
  }

}

impl PartialEq for Offer {
  fn eq(&self, other: &Self) -> bool {
    if self.agent_name != other.agent_name {
      return false;
    }
    if self.resolved_conditions.len() != other.resolved_conditions.len() {
      return false;
    }
    for self_condition in &self.resolved_conditions {
      let mut found = false;
      for other_condition in &other.resolved_conditions {
        if self_condition == other_condition {
          found = true;
          break;
        }
      }
      if !found {
        return false;
      }
    }
    true
  }
}
impl Eq for Offer {}

#[cfg(test)]
mod tests_offer {
  use super::*;

  #[test]
  fn test_eq() {
    assert_eq!(
      Offer::new("a"),
      Offer::new("a"),
    );
    assert_eq!(
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("c"))),
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("c"))),
    );
    assert_ne!(
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("c"))),
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("d"))),
    );
    assert_ne!(
      Offer::new_conditional("a", vec!(Resolution::new("b"))),
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("c"))),
    );
    assert_eq!(
      Offer::new_conditional("a", vec!(Resolution::new("b"), Resolution::new("c"))),
      Offer::new_conditional("a", vec!(Resolution::new("c"), Resolution::new("b"))),
    );
  }
}
