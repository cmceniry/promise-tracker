use crate::components::{mark_spans, merge_spans};
use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Byte ranges of a behavior name that a substitution filled in. Only ever
/// used to draw the name with those values wrapped in `<>`.
pub type Spans = Vec<(usize, usize)>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Resolution {
    behavior_name: String,
    satisfying_offers: Vec<Offer>,
    unsatisfying_offers: Vec<Offer>,
    /// The parts of `behavior_name` that whoever needs it filled in: its own
    /// `{{!self}}` or Instance binding, a value the promise's goal bound, or
    /// one a sibling condition found. Empty when the name was written out.
    ///
    /// Display only. It depends on how the goal was reached, not only on its
    /// text, so it takes no part in equality, and it is never serialized:
    /// byte offsets are of no use outside this process, and the serialized
    /// form stays as it always was.
    #[serde(skip)]
    bound: Spans,
    /// Some promise for this goal had more readings than the cap
    /// (`MAX_CONDITION_READINGS`), so not every one was tried: an answer of
    /// "not kept" may be the cap's, not the contract's. Left out of equality
    /// and `Debug`, and serialized only when set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cut_short: bool,
}

impl Resolution {
    pub fn new(behavior_name: &str) -> Resolution {
        Resolution {
            behavior_name: String::from(behavior_name),
            satisfying_offers: vec![],
            unsatisfying_offers: vec![],
            bound: vec![],
            cut_short: false,
        }
    }

    /// This resolution, marked as one where the reading cap dropped some of
    /// a promise's readings.
    pub fn with_cut_short(mut self) -> Resolution {
        self.cut_short = true;
        self
    }

    /// Did the reading cap drop readings of some promise for this goal?
    pub fn is_cut_short(&self) -> bool {
        self.cut_short
    }

    /// This resolution, with the parts of its name a substitution filled in.
    pub fn with_bound(mut self, bound: Spans) -> Resolution {
        self.bound = bound;
        self
    }

    /// The parts of the name whoever needs it filled in. Display only.
    pub fn get_bound(&self) -> &[(usize, usize)] {
        &self.bound
    }

    /// The name as whoever needs it wrote it, substituted values in `<>`:
    /// `<A1> reads R1`. The plain name when nothing was substituted. Display
    /// only; never a behavior name.
    pub fn marked_name(&self) -> String {
        mark_spans(&self.behavior_name, &self.bound)
    }

    /// The name with what either side filled in marked: the needing side as
    /// in [`Resolution::marked_name`], and every promise that names it
    /// through a variable. An offer answering a gathered row keeps a behavior
    /// of its own and adds nothing here.
    pub fn marked_name_with_offers(&self) -> String {
        let mut spans = self.bound.clone();
        for offer in self
            .satisfying_offers
            .iter()
            .chain(&self.unsatisfying_offers)
        {
            if offer.behavior.is_none() {
                spans = merge_spans(&spans, &offer.bound);
            }
        }
        mark_spans(&self.behavior_name, &spans)
    }

    /// The behavior `offer` keeps for this resolution, with what both sides
    /// filled in marked. For a gathered row that is the concrete behavior the
    /// offer matched, marked where the goal's variables fell.
    pub fn marked_offer(&self, offer: &Offer) -> String {
        mark_spans(
            offer.get_behavior().unwrap_or(&self.behavior_name),
            &offer.kept_spans(&self.bound),
        )
    }

    /// The whole resolution with its marks, one line per name: this goal as
    /// [`Resolution::marked_name`], then under it each offer as
    /// `agent: marked_offer`, satisfying ones first, each followed by its
    /// conditions the same way, two spaces further in per level.
    ///
    /// Equality and `Debug` leave the display-only spans out, so this is how
    /// a test says what they are.
    #[doc(hidden)]
    pub fn marked_outline(&self) -> Vec<String> {
        let mut out = vec![];
        self.outline_into(0, &mut out);
        out
    }

    fn outline_into(&self, depth: usize, out: &mut Vec<String>) {
        out.push(format!("{}{}", "  ".repeat(depth), self.marked_name()));
        for offer in self
            .satisfying_offers
            .iter()
            .chain(&self.unsatisfying_offers)
        {
            out.push(format!(
                "{}{}: {}",
                "  ".repeat(depth + 1),
                offer.agent_name,
                self.marked_offer(offer)
            ));
            for condition in &offer.resolved_conditions {
                condition.outline_into(depth + 2, out);
            }
        }
    }

    pub(crate) fn offers_mut(&mut self) -> impl Iterator<Item = &mut Offer> {
        self.satisfying_offers
            .iter_mut()
            .chain(self.unsatisfying_offers.iter_mut())
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

    /// This resolution without the alternatives it does not need: wherever a
    /// behavior is kept, the offers that fail to keep it are dropped, each
    /// with everything it rests on. This is the "Fold: Unneeded
    /// alternatives" display option (`--fold-alternatives`).
    ///
    /// The rule holds at every depth, inside an unsatisfied want's chain
    /// too: a condition that is kept loses its failing offers even when the
    /// promise it belongs to is broken for another reason. A behavior nobody
    /// keeps still shows every offer, and every offer that keeps a behavior
    /// stays. An offer answering a gathered row is an offer like any other.
    ///
    /// Display only: whether anything is satisfied never changes, and doing
    /// it twice changes nothing more.
    pub fn without_unneeded_alternatives(mut self) -> Resolution {
        if self.is_satisfied() {
            self.unsatisfying_offers.clear();
        }
        for offer in self.offers_mut() {
            offer.resolved_conditions = std::mem::take(&mut offer.resolved_conditions)
                .into_iter()
                .map(Resolution::without_unneeded_alternatives)
                .collect();
        }
        self
    }

    /// Returns the behavior name
    pub fn get_behavior_name(&self) -> &str {
        &self.behavior_name
    }

    /// Returns a reference to the satisfying offers
    pub fn get_satisfying_offers(&self) -> &[Offer] {
        &self.satisfying_offers
    }

    /// Returns a reference to the unsatisfying offers
    pub fn get_unsatisfying_offers(&self) -> &[Offer] {
        &self.unsatisfying_offers
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

/// Written out so that it prints what it always has: CLI `simulate` shows a
/// resolution this way, and the display-only spans are not part of it.
impl std::fmt::Debug for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Resolution")
            .field("behavior_name", &self.behavior_name)
            .field("satisfying_offers", &self.satisfying_offers)
            .field("unsatisfying_offers", &self.unsatisfying_offers)
            .finish()
    }
}

/// Equal when the name and the offers are, in any order. `bound` is left out:
/// it only says how to draw the name, and the same goal reached two ways is
/// the same resolution.
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
    fn test_without_unneeded_alternatives_satisfied_root_drops_broken_offer_and_subtree() {
        let r = Resolution::new("b1")
            .add_satisfying_offer(Offer::new("good"))
            .add_unsatisfying_offer(Offer::new_conditional(
                "alt",
                vec![
                    Resolution::new("c").add_satisfying_offer(Offer::new("cprov")),
                    Resolution::new("d"),
                ],
            ));
        assert_eq!(
            r.without_unneeded_alternatives(),
            Resolution::new("b1").add_satisfying_offer(Offer::new("good"))
        );
    }

    #[test]
    fn test_without_unneeded_alternatives_keeps_every_satisfying_offer() {
        let r = Resolution::new("b1")
            .add_satisfying_offer(Offer::new("a1"))
            .add_satisfying_offer(Offer::new_conditional(
                "a2",
                vec![Resolution::new("c")
                    .add_satisfying_offer(Offer::new("c1"))
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "c2",
                        vec![Resolution::new("d")],
                    ))],
            ))
            .add_unsatisfying_offer(Offer::new_conditional("a3", vec![Resolution::new("e")]));
        assert_eq!(
            r.without_unneeded_alternatives(),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_satisfying_offer(Offer::new_conditional(
                    "a2",
                    vec![Resolution::new("c").add_satisfying_offer(Offer::new("c1"))],
                ))
        );
    }

    #[test]
    fn test_without_unneeded_alternatives_unsatisfied_root_keeps_offers_prunes_nested() {
        let r = Resolution::new("top").add_unsatisfying_offer(Offer::new_conditional(
            "p",
            vec![
                Resolution::new("z1")
                    .add_satisfying_offer(Offer::new("az1"))
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "ay1",
                        vec![Resolution::new("y1")],
                    )),
                Resolution::new("z2"),
            ],
        ));
        let pruned = r.without_unneeded_alternatives();
        assert!(!pruned.is_satisfied());
        assert_eq!(
            pruned,
            Resolution::new("top").add_unsatisfying_offer(Offer::new_conditional(
                "p",
                vec![
                    Resolution::new("z1").add_satisfying_offer(Offer::new("az1")),
                    Resolution::new("z2"),
                ],
            ))
        );

        // nobody keeps it: every offer stays, as it was
        let all_broken = Resolution::new("b1")
            .add_unsatisfying_offer(Offer::new_conditional("a1", vec![Resolution::new("c")]))
            .add_unsatisfying_offer(Offer::new_conditional("a2", vec![Resolution::new("d")]));
        assert_eq!(all_broken.clone().without_unneeded_alternatives(), all_broken);
    }

    #[test]
    fn test_without_unneeded_alternatives_no_offers_unchanged() {
        assert_eq!(
            Resolution::new("b1").without_unneeded_alternatives(),
            Resolution::new("b1")
        );
    }

    #[test]
    fn test_without_unneeded_alternatives_open_goal_offers_are_ordinary() {
        let open = Resolution::new("{{who}} is up")
            .add_satisfying_offer(Offer::new("a").answering("a is up"))
            .add_unsatisfying_offer(
                Offer::new_conditional("b", vec![Resolution::new("power")]).answering("b is up"),
            );
        assert_eq!(
            open.without_unneeded_alternatives(),
            Resolution::new("{{who}} is up")
                .add_satisfying_offer(Offer::new("a").answering("a is up"))
        );

        // no candidate keeps it: both stay
        let broken = Resolution::new("{{who}} is up")
            .add_unsatisfying_offer(
                Offer::new_conditional("a", vec![Resolution::new("fuel")]).answering("a is up"),
            )
            .add_unsatisfying_offer(
                Offer::new_conditional("b", vec![Resolution::new("power")]).answering("b is up"),
            );
        assert_eq!(broken.clone().without_unneeded_alternatives(), broken);
    }

    #[test]
    fn test_without_unneeded_alternatives_idempotent_and_keeps_satisfaction() {
        let cases = vec![
            Resolution::new("b1"),
            Resolution::new("b1").add_satisfying_offer(Offer::new("a1")),
            Resolution::new("b1")
                .add_satisfying_offer(Offer::new("a1"))
                .add_unsatisfying_offer(Offer::new_conditional("a2", vec![Resolution::new("c")])),
            Resolution::new("top").add_unsatisfying_offer(Offer::new_conditional(
                "p",
                vec![
                    Resolution::new("z1")
                        .add_satisfying_offer(Offer::new("az1"))
                        .add_unsatisfying_offer(Offer::new_conditional(
                            "ay1",
                            vec![Resolution::new("y1")],
                        )),
                    Resolution::new("z2"),
                ],
            )),
        ];
        for case in cases {
            let once = case.clone().without_unneeded_alternatives();
            assert_eq!(once.is_satisfied(), case.is_satisfied());
            assert_eq!(once.clone().without_unneeded_alternatives(), once);
        }
    }

    #[test]
    fn test_without_unneeded_alternatives_keeps_display_spans() {
        let r = Resolution::new("a1 reads r1")
            .with_bound(vec![(0, 2)])
            .add_satisfying_offer(Offer::new("a1"))
            .add_unsatisfying_offer(Offer::new("a2"));
        assert_eq!(r.without_unneeded_alternatives().get_bound(), &[(0, 2)]);
    }

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

#[derive(Clone, Serialize, Deserialize)]
pub struct Offer {
    agent_name: String,
    resolved_conditions: Vec<Resolution>,
    /// The concrete behavior this offer keeps, when the row it answers
    /// still holds a declared variable (`{{doc}} shows usage`) and so names
    /// none itself. Absent for a
    /// concrete goal, whose name already says it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    behavior: Option<String>,
    /// The parts of the behavior this offer keeps that the promise names
    /// through a variable: its agent's `{{!self}}` or Instance binding, or
    /// what a parameterized name took from the goal. Display only, and like
    /// [`Resolution`]'s no part of equality or of the serialized form.
    #[serde(skip)]
    bound: Spans,
    /// Under a gathered row, the parts of `behavior` the row filled in or
    /// its variables matched. Display only; no part of equality or of the
    /// serialized form.
    #[serde(skip)]
    matched: Spans,
}

impl Offer {
    pub fn new(agent_name: &str) -> Offer {
        Offer {
            agent_name: String::from(agent_name),
            resolved_conditions: vec![],
            behavior: None,
            bound: vec![],
            matched: vec![],
        }
    }

    pub fn new_conditional(agent_name: &str, resolved_conditions: Vec<Resolution>) -> Offer {
        Offer {
            agent_name: String::from(agent_name),
            resolved_conditions,
            behavior: None,
            bound: vec![],
            matched: vec![],
        }
    }

    /// This offer, as the answer to a gathered row by way of the concrete
    /// `behavior` it matched.
    pub fn answering(mut self, behavior: &str) -> Offer {
        self.behavior = Some(behavior.to_string());
        self
    }

    /// This offer, with the parts of the behavior it keeps that its promise
    /// names through a variable.
    pub fn with_bound(mut self, bound: Spans) -> Offer {
        self.bound = bound;
        self
    }

    /// This offer, with the parts of the behavior it keeps that the open
    /// goal it answers filled in or matched.
    pub fn with_matched(mut self, matched: Spans) -> Offer {
        self.matched = matched;
        self
    }

    /// The parts of the kept behavior the promise names through a variable.
    pub fn get_bound(&self) -> &[(usize, usize)] {
        &self.bound
    }

    /// The parts of the kept behavior a gathered row filled in or matched.
    pub fn get_matched(&self) -> &[(usize, usize)] {
        &self.matched
    }

    /// The substituted parts of the behavior this offer keeps, from both
    /// sides. `needed` is what whoever needs the goal filled in on its name
    /// ([`Resolution::get_bound`]); under a gathered row the offer keeps a
    /// behavior of its own and carries the goal's side itself.
    pub fn kept_spans(&self, needed: &[(usize, usize)]) -> Spans {
        if self.behavior.is_some() {
            merge_spans(&self.matched, &self.bound)
        } else {
            merge_spans(needed, &self.bound)
        }
    }

    /// The concrete behavior this offer keeps, when the goal was open.
    pub fn get_behavior(&self) -> Option<&str> {
        self.behavior.as_deref()
    }

    /// Returns the agent name
    pub fn get_agent_name(&self) -> &str {
        &self.agent_name
    }

    /// Returns a reference to the resolved conditions
    pub fn get_resolved_conditions(&self) -> &[Resolution] {
        &self.resolved_conditions
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

/// As [`Resolution`]'s: without the display-only spans.
impl std::fmt::Debug for Offer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Offer")
            .field("agent_name", &self.agent_name)
            .field("resolved_conditions", &self.resolved_conditions)
            .field("behavior", &self.behavior)
            .finish()
    }
}

/// Equal when the agent, the kept behavior and the conditions are, in any
/// order. `bound` and `matched` are left out: they only say how to draw the
/// behavior.
impl PartialEq for Offer {
    fn eq(&self, other: &Self) -> bool {
        if self.agent_name != other.agent_name {
            return false;
        }
        if self.behavior != other.behavior {
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
    fn test_display_spans_are_not_identity_and_not_serialized() {
        let plain = Resolution::new("a1 reads r1").add_satisfying_offer(Offer::new("s1"));
        let marked = Resolution::new("a1 reads r1")
            .with_bound(vec![(0, 2)])
            .add_satisfying_offer(Offer::new("s1").with_bound(vec![(9, 11)]));
        assert_eq!(plain, marked);

        assert_eq!(plain.marked_name(), "a1 reads r1");
        assert_eq!(marked.marked_name(), "<a1> reads r1");
        assert_eq!(marked.marked_name_with_offers(), "<a1> reads <r1>");
        assert_eq!(
            marked.marked_offer(&marked.get_satisfying_offers()[0]),
            "<a1> reads <r1>"
        );
        // Under a gathered row the offer carries the goal's side itself.
        let open = Resolution::new("{{who}} reads r1")
            .with_bound(vec![(14, 16)])
            .add_satisfying_offer(
                Offer::new("s1")
                    .answering("a1 reads r1")
                    .with_matched(vec![(0, 2), (9, 11)]),
            );
        assert_eq!(open.marked_name(), "{{who}} reads <r1>");
        assert_eq!(open.marked_name_with_offers(), "{{who}} reads <r1>");
        assert_eq!(
            open.marked_offer(&open.get_satisfying_offers()[0]),
            "<a1> reads <r1>"
        );

        // Printed for CLI `simulate` as it always was, spans or not.
        assert_eq!(format!("{:?}", marked), format!("{:?}", plain));
        assert_eq!(format!("{:#?}", marked), format!("{:#?}", plain));
        assert_eq!(
            format!("{:?}", Offer::new("s1")),
            "Offer { agent_name: \"s1\", resolved_conditions: [], behavior: None }"
        );

        // Never serialized, set or not: byte offsets mean nothing to
        // whoever reads the JSON, and the form is as it always was.
        let json = serde_json::to_string(&plain).unwrap();
        assert!(
            !json.contains("bound") && !json.contains("matched"),
            "{}",
            json
        );
        assert_eq!(serde_json::to_string(&marked).unwrap(), json);
        assert_eq!(serde_json::to_string(&open).unwrap().contains("matched"), false);
        let back: Resolution =
            serde_json::from_str(&serde_json::to_string(&marked).unwrap()).unwrap();
        assert_eq!(back, marked);
        assert_eq!(back.marked_name_with_offers(), "a1 reads r1");

        // The outline is how a test reads the spans.
        assert_eq!(
            marked.marked_outline(),
            vec!["<a1> reads r1", "  s1: <a1> reads <r1>"]
        );
        assert_eq!(
            open.marked_outline(),
            vec!["{{who}} reads <r1>", "  s1: <a1> reads <r1>"]
        );
        assert_eq!(plain.marked_outline(), vec!["a1 reads r1", "  s1: a1 reads r1"]);
    }

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
