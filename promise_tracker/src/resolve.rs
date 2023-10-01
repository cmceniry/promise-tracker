use colored::Colorize;

#[derive(Debug)]
pub struct Resolution {
    behavior_name: String,
    satisfying_offers: Vec<Offer>,
    unsatisfying_offers: Vec<Offer>,
}

impl Resolution {
    pub fn new(behavior_name: &str) -> Resolution {
        Resolution {
            behavior_name: String::from(behavior_name),
            satisfying_offers: vec![],
            unsatisfying_offers: vec![],
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

    // resolve strings is of the format
    // behavior |-> offerer ...
    // offer is of the format
    //
    pub fn to_colorized_compressed_strings(&self) -> Vec<String> {
        if self.satisfying_offers.len() == 0 && self.unsatisfying_offers.len() == 0 {
            return vec![format!(
                "{} {} {}",
                self.behavior_name.red(),
                "|->".red(),
                "?".red()
            )];
        }
        let (colorized_behavior, spacer_behavior) = if self.is_satisfied() {
            (
                self.behavior_name.green(),
                " ".repeat(self.behavior_name.len()).green(),
            )
        } else {
            (
                self.behavior_name.red(),
                " ".repeat(self.behavior_name.len()).red(),
            )
        };
        let mut ret: Vec<String> = vec![];
        for offer in &self.satisfying_offers {
            let colorized_agent = "|->".green();
            let spacer_offer = "   ".green();
            let mut offer_lines = offer.to_colorized_compressed_strings();
            offer_lines[0] = format!(
                "{} {} {}",
                if ret.len() == 0 {
                    &colorized_behavior
                } else {
                    &spacer_behavior
                },
                &colorized_agent,
                &offer_lines[0],
            );
            for i in 1..offer_lines.len() {
                offer_lines[i] =
                    format!("{} {} {}", &spacer_behavior, &spacer_offer, &offer_lines[i]);
            }
            ret.extend(offer_lines);
        }
        for offer in &self.unsatisfying_offers {
            let colorized_agent = "|->".red();
            let spacer_offer = "   ".red();
            let mut offer_lines = offer.to_colorized_compressed_strings();
            offer_lines[0] = format!(
                "{} {} {}",
                if ret.len() == 0 {
                    &colorized_behavior
                } else {
                    &spacer_behavior
                },
                &colorized_agent,
                &offer_lines[0],
            );
            for i in 1..offer_lines.len() {
                offer_lines[i] =
                    format!("{} {} {}", &spacer_behavior, &spacer_offer, &offer_lines[i]);
            }
            ret.extend(offer_lines);
        }
        ret
    }

    pub fn to_strings_compressed(&self, use_color: bool) -> Vec<String> {
        if self.satisfying_offers.len() == 0 && self.unsatisfying_offers.len() == 0 {
            return vec![format!(
                "{} {} {}",
                self.behavior_name,
                if use_color {
                    "|->".red().to_string()
                } else {
                    "|->".to_string()
                },
                "?"
            )];
        }
        let mut ret = vec![];
        for offer in &self.satisfying_offers {
            let mut children = offer.to_strings_compressed(use_color);
            children[0] = format!(
                "{} {} {}",
                &" ".repeat(self.behavior_name.len()),
                if use_color {
                    "|->".green().to_string()
                } else {
                    "|->".to_string()
                },
                children[0]
            );
            for child in &mut children[1..] {
                child.insert_str(0, &" ".repeat(self.behavior_name.len() + 5));
            }
            ret.extend(children);
        }
        for offer in &self.unsatisfying_offers {
            let mut children = offer.to_strings_compressed(use_color);
            children[0] = format!(
                "{} {} {}",
                &" ".repeat(self.behavior_name.len()),
                if use_color {
                    "|->".red().to_string()
                } else {
                    "|->".to_string()
                },
                children[0]
            );
            for child in &mut children[1..] {
                child.insert_str(0, &" ".repeat(self.behavior_name.len() + 5));
            }
            ret.extend(children);
        }
        ret[0].replace_range(0..self.behavior_name.len(), &self.behavior_name);
        ret
    }

    pub fn to_colorized_strings(&self) -> Vec<String> {
        if self.satisfying_offers.len() == 0 && self.unsatisfying_offers.len() == 0 {
            return vec![
                self.behavior_name.red().to_string(),
                format!("  {} {}", "|->".red(), "?".red()),
            ];
        };
        let mut ret = vec![if self.is_satisfied() {
            self.behavior_name.green().to_string()
        } else {
            self.behavior_name.red().to_string()
        }];
        for offer in &self.satisfying_offers {
            let mut offer_lines = offer.to_colorized_strings();
            offer_lines[0] = format!("  {} {}", "|->".green(), &offer_lines[0]);
            for i in 1..offer_lines.len() {
                offer_lines[i] = format!("  {}", &offer_lines[i]);
            }
            ret.extend(offer_lines);
        }
        for offer in &self.unsatisfying_offers {
            let mut offer_lines = offer.to_colorized_strings();
            offer_lines[0] = format!("  {} {}", "|->".red(), &offer_lines[0]);
            for i in 1..offer_lines.len() {
                offer_lines[i] = format!("  {}", &offer_lines[i]);
            }
            ret.extend(offer_lines);
        }
        ret
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
        if self.unsatisfying_offers.len() != other.unsatisfying_offers.len() {
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
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a2"))
                .add_satisfying_offer(Offer::new("a1"))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2")))),
        );
        // mismatch in satisfying offers count
        assert_ne!(
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
        );
        // mismatch in unsatisfying offers count
        assert_ne!(
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2")),
        );
        // mismatch in unsatisfying offers' conditions
        assert_ne!(
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b3")))),
        );
        // mismatch in satisfying offers
        assert_ne!(
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a3"))
                .add_unsatisfying_offer(Offer::new_conditional("a3", vec!(Resolution::new("b2"))))
                .add_unsatisfying_offer(Offer::new_conditional("a4", vec!(Resolution::new("b2")))),
        );
        // unequal in unsatifying offers
        assert_ne!(
            Resolution::new("b1"),
            Resolution::new("b1").add_unsatisfying_offer(Offer::new("a1"))
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
        Offer {
            agent_name: String::from(agent_name),
            resolved_conditions: vec![],
        }
    }

    pub fn new_conditional(agent_name: &str, resolved_conditions: Vec<Resolution>) -> Offer {
        Offer {
            agent_name: String::from(agent_name),
            resolved_conditions,
        }
    }

    pub fn to_strings_compressed(&self, use_color: bool) -> Vec<String> {
        if self.resolved_conditions.len() == 0 {
            return vec![format!("{}", self.agent_name)];
        }
        let mut ret = vec![];
        for condition in &self.resolved_conditions {
            let mut children = condition.to_strings_compressed(use_color);
            children[0].insert_str(0, &format!("{} &-> ", &" ".repeat(self.agent_name.len())));
            for child in &mut children[1..] {
                child.insert_str(0, &" ".repeat(self.agent_name.len() + 5));
            }
            ret.extend(children);
        }
        ret[0].replace_range(0..self.agent_name.len(), &self.agent_name);
        ret
    }

    pub fn to_colorized_compressed_strings(&self) -> Vec<String> {
        if self.resolved_conditions.len() == 0 {
            return vec![format!("{}", self.agent_name.green())];
        }
        let satisfied = self.resolved_conditions.iter().all(|c| c.is_satisfied());
        let (colorized_agent, spacer_agent) = if satisfied {
            (
                self.agent_name.green(),
                " ".repeat(self.agent_name.len()).green(),
            )
        } else {
            (
                self.agent_name.red(),
                " ".repeat(self.agent_name.len()).red(),
            )
        };
        let mut ret = vec![];
        for condition in &self.resolved_conditions {
            let (colorized_condition, spacer_condition) = if condition.is_satisfied() {
                ("&->".green(), "   ".green())
            } else {
                ("&->".red(), "   ".red())
            };
            let mut condition_lines = condition.to_colorized_compressed_strings();
            condition_lines[0] = format!(
                "{} {} {}",
                if ret.len() == 0 {
                    &colorized_agent
                } else {
                    &spacer_agent
                },
                &colorized_condition,
                &condition_lines[0],
            );
            for i in 1..condition_lines.len() {
                condition_lines[i] = format!(
                    "{} {} {}",
                    &spacer_agent, &spacer_condition, &condition_lines[i],
                )
            }
            ret.extend(condition_lines);
        }
        ret
    }

    pub fn to_colorized_strings(&self) -> Vec<String> {
        if self.resolved_conditions.len() == 0 {
            return vec![format!("{}", self.agent_name.green())];
        };
        let satisfied = self.resolved_conditions.iter().all(|c| c.is_satisfied());
        let mut ret = vec![if satisfied {
            self.agent_name.green().to_string()
        } else {
            self.agent_name.red().to_string()
        }];
        for condition in &self.resolved_conditions {
            let mut condition_lines = condition.to_colorized_strings();
            condition_lines[0] = format!(
                "  {} {}",
                if condition.is_satisfied() {
                    "&->".green()
                } else {
                    "&->".red()
                },
                &condition_lines[0]
            );
            for i in 1..condition_lines.len() {
                condition_lines[i] = format!("  {}", &condition_lines[i]);
            }
            ret.extend(condition_lines);
        }
        ret
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
        assert_eq!(Offer::new("a"), Offer::new("a"),);
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

    #[test]
    fn test_simple_pretty_string() {
        assert_eq!(
            Resolution::new("b1").to_strings_compressed(false),
            vec!["b1 |-> ?".to_string(),]
        );

        assert_eq!(
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new("a2"))
                .to_strings_compressed(false),
            vec!["b1 |-> a1".to_string(), "   |-> a2".to_string(),],
        );

        assert_eq!(
            Offer::new("a1").to_strings_compressed(false),
            vec!["a1".to_string()],
        );

        assert_eq!(
            Offer::new_conditional("a1", vec![Resolution::new("c1"), Resolution::new("c2")])
                .to_strings_compressed(false),
            vec!["a1 &-> c1 |-> ?".to_string(), "   &-> c2 |-> ?".to_string()],
        );
    }

    #[test]
    fn test_deep_pretty_string() {
        let r = Resolution::new("b1")
            .add_unsatisfying_offer(Offer::new_conditional(
                "a1",
                vec![
                    Resolution::new("b2").add_satisfying_offer(Offer::new_conditional(
                        "a2",
                        vec![
                            Resolution::new("ba2a").add_satisfying_offer(Offer::new("a2a")),
                            Resolution::new("ba2b").add_satisfying_offer(Offer::new("a2b")),
                        ],
                    )),
                    Resolution::new("b3"),
                ],
            ))
            .add_satisfying_offer(Offer::new_conditional(
                "a4",
                vec![Resolution::new("b5")
                    .add_satisfying_offer(Offer::new_conditional("a5", vec![]))],
            ))
            .add_unsatisfying_offer(Offer::new_conditional("a6", vec![Resolution::new("b7")]))
            .add_unsatisfying_offer(Offer::new_conditional(
                "a8",
                vec![
                    Resolution::new("b9").add_unsatisfying_offer(Offer::new_conditional(
                        "a9",
                        vec![Resolution::new("b10").add_unsatisfying_offer(
                            Offer::new_conditional("a10", vec![Resolution::new("b11")]),
                        )],
                    )),
                ],
            ));
        assert_eq!(
            r.to_strings_compressed(false),
            vec![
                "b1 |-> a4 &-> b5 |-> a5".to_string(),
                "   |-> a1 &-> b2 |-> a2 &-> ba2a |-> a2a".to_string(),
                "                        &-> ba2b |-> a2b".to_string(),
                "          &-> b3 |-> ?".to_string(),
                "   |-> a6 &-> b7 |-> ?".to_string(),
                "   |-> a8 &-> b9 |-> a9 &-> b10 |-> a10 &-> b11 |-> ?".to_string(),
            ],
        );
        assert_eq!(
            r.to_strings_compressed(true),
            vec![
                format!("b1 {} a4 &-> b5 {} a5", "|->".green(), "|->".green()),
                format!(
                    "   {} a1 &-> b2 {} a2 &-> ba2a {} a2a",
                    "|->".red(),
                    "|->".green(),
                    "|->".green()
                ),
                format!("                        &-> ba2b {} a2b", "|->".green()),
                format!("          &-> b3 {} ?", "|->".red()),
                format!("   {} a6 &-> b7 {} ?", "|->".red(), "|->".red()),
                format!(
                    "   {} a8 &-> b9 {} a9 &-> b10 {} a10 &-> b11 {} ?",
                    "|->".red(),
                    "|->".red(),
                    "|->".red(),
                    "|->".red()
                )
            ],
        )
    }

    #[test]
    fn test_to_colorized_compressed_string() {
        assert_eq!(
            Offer::new("a1").to_colorized_compressed_strings(),
            vec![format!("{}", "a1".green(),)]
        );

        assert_eq!(
            Resolution::new("b1").to_colorized_compressed_strings(),
            vec![format!("{} {} {}", "b1".red(), "|->".red(), "?".red())],
        );

        assert_eq!(
            Offer::new_conditional("a1", vec![Resolution::new("b1")])
                .to_colorized_compressed_strings(),
            vec![format!(
                "{} {} {} {} {}",
                "a1".red(),
                "&->".red(),
                "b1".red(),
                "|->".red(),
                "?".red()
            )],
        );

        assert_eq!(
            Offer::new_conditional(
                "a1",
                vec![Resolution::new("b1").add_satisfying_offer(Offer::new("a2"))]
            )
            .to_colorized_compressed_strings(),
            vec![format!(
                "{} {} {} {} {}",
                "a1".green(),
                "&->".green(),
                "b1".green(),
                "|->".green(),
                "a2".green(),
            )],
        );

        assert_eq!(
            Offer::new_conditional(
                "a1",
                vec![
                    Resolution::new("b2").add_satisfying_offer(Offer::new("a2")),
                    Resolution::new("b3"),
                ]
            )
            .to_colorized_compressed_strings(),
            vec![
                format!(
                    "{} {} {} {} {}",
                    "a1".red(),
                    "&->".green(),
                    "b2".green(),
                    "|->".green(),
                    "a2".green(),
                ),
                format!(
                    "{} {} {} {} {}",
                    "  ".red(),
                    "&->".red(),
                    "b3".red(),
                    "|->".red(),
                    "?".red()
                ),
            ]
        );
    }

    #[test]
    fn test_to_colorized_string() {
        assert_eq!(
            Offer::new("a1").to_colorized_strings(),
            vec!["a1".green().to_string(),],
        );

        assert_eq!(
            Resolution::new("b1").to_colorized_strings(),
            vec![
                "b1".red().to_string(),
                format!("  {} {}", "|->".red(), "?".red())
            ],
        );

        assert_eq!(
            Offer::new_conditional("a1", vec![Resolution::new("b2")]).to_colorized_strings(),
            vec![
                "a1".red().to_string(),
                format!("  {} {}", "&->".red(), "b2".red()),
                format!("    {} {}", "|->".red(), "?".red()),
            ],
        );

        assert_eq!(
            Offer::new_conditional("a1", vec![Resolution::new("b2"), Resolution::new("b3")])
                .to_colorized_strings(),
            vec![
                "a1".red().to_string(),
                format!("  {} {}", "&->".red(), "b2".red()),
                format!("    {} {}", "|->".red(), "?".red()),
                format!("  {} {}", "&->".red(), "b3".red()),
                format!("    {} {}", "|->".red(), "?".red()),
            ],
        );

        assert_eq!(
            Offer::new_conditional(
                "a1",
                vec![
                    Resolution::new("b2").add_satisfying_offer(Offer::new("a2")),
                    Resolution::new("b3"),
                ]
            )
            .to_colorized_strings(),
            vec![
                "a1".red().to_string(),
                format!("  {} {}", "&->".green(), "b2".green()),
                format!("    {} {}", "|->".green(), "a2".green()),
                format!("  {} {}", "&->".red(), "b3".red()),
                format!("    {} {}", "|->".red(), "?".red()),
            ],
        );
    }
}
