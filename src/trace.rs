//! One want, read as a trail.
//!
//! [`Tracker::resolve`] answers a want with a tree: the offers for it, each
//! offer's conditions, their offers, and so on down. Drawn as a tree it repeats
//! itself, because the same promise is reached from many branches; in the
//! Kubernetes example `kubernetes-api` would be drawn dozens of times under a
//! single want. A trail is that tree with every promise placed once. The first
//! time a promise is reached it gets a card at that depth; every later time, the
//! step that reached it holds a reference to the card instead of a copy.
//!
//! Depth is a column in the view: the want, who promises it, what that rests
//! on, and so on to the right.

use crate::resolve::{Offer, Resolution};
use crate::Tracker;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// What a card stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    /// The want that starts the trail; its agent is the one wanting.
    Want,
    /// A promise: a behavior, and the agent making it.
    Offer,
    /// A behavior nobody promises.
    Missing,
}

/// One thing a card rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Need {
    pub behavior: String,
    /// Index into [`Trail::cards`].
    pub card: usize,
    /// The card was already on the trail when this step reached it, so the
    /// view shows a pointer back to it rather than a second copy.
    pub reference: bool,
    /// Met by the same agent that holds this need: a condition a provider
    /// satisfies itself.
    pub own: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailCard {
    pub id: usize,
    /// Column: 0 is the want, 1 whoever promises it, and so on.
    pub depth: usize,
    pub kind: CardKind,
    pub behavior: String,
    /// The agent making the promise, or wanting it on the want card. None
    /// when nobody promises the behavior.
    pub agent: Option<String>,
    pub satisfied: bool,
    pub needs: Vec<Need>,
}

/// A want and everything it rests on, each promise placed once.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Trail {
    pub agent: String,
    pub behavior: String,
    pub satisfied: bool,
    /// Card 0 is the want. Cards are in the order they were reached, which is
    /// depth first through the resolution, so within a column the cards hanging
    /// under one parent sit together.
    pub cards: Vec<TrailCard>,
}

impl Trail {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Number of columns the trail needs.
    pub fn width(&self) -> usize {
        self.cards.iter().map(|c| c.depth + 1).max().unwrap_or(0)
    }

    /// The cards at one depth, in trail order.
    pub fn column(&self, depth: usize) -> Vec<&TrailCard> {
        self.cards.iter().filter(|c| c.depth == depth).collect()
    }

    /// Cards that are not held: broken offers and missing behaviors.
    pub fn broken(&self) -> Vec<&TrailCard> {
        self.cards
            .iter()
            .filter(|c| c.kind != CardKind::Want && !c.satisfied)
            .collect()
    }

    /// The trail with every promise an agent makes to itself folded away.
    ///
    /// A need met by the agent that holds it is spliced out: the card it
    /// pointed at goes, and whatever that card needed from other agents hangs
    /// under the holder instead. Cards nothing reaches any more are dropped,
    /// depths are re-measured from the want, and references are re-marked, so
    /// the result reads the same as a trail built that way from the start.
    pub fn without_own_promises(self) -> Trail {
        if self.cards.is_empty() {
            return self;
        }

        // Resolve each card's needs with own needs replaced by what they led
        // to. A chain of own needs (a provider meeting its own condition with a
        // promise that itself rests on another of its own) is followed to the
        // first promise from someone else, with a seen set against loops.
        let mut spliced: Vec<Vec<Need>> = Vec::with_capacity(self.cards.len());
        for card in &self.cards {
            let mut out: Vec<Need> = vec![];
            let mut seen: HashSet<usize> = HashSet::new();
            let mut queue: VecDeque<&Need> = card.needs.iter().collect();
            while let Some(need) = queue.pop_front() {
                if need.own {
                    if seen.insert(need.card) {
                        queue.extend(self.cards[need.card].needs.iter());
                    }
                } else {
                    out.push(need.clone());
                }
            }
            spliced.push(out);
        }

        // Walk from the want again: what is reached, at what depth, and which
        // step reached it first.
        let mut depth: HashMap<usize, usize> = HashMap::new();
        let mut order: Vec<usize> = vec![];
        let mut queue: VecDeque<usize> = VecDeque::new();
        depth.insert(0, 0);
        order.push(0);
        queue.push_back(0);
        while let Some(id) = queue.pop_front() {
            let d = depth[&id];
            for need in &spliced[id] {
                if !depth.contains_key(&need.card) {
                    depth.insert(need.card, d + 1);
                    order.push(need.card);
                    queue.push_back(need.card);
                }
            }
        }

        // Keep the original card order among the survivors so columns still
        // read depth first, then renumber.
        let mut keep: Vec<usize> = order;
        keep.sort();
        let new_id: HashMap<usize, usize> = keep
            .iter()
            .enumerate()
            .map(|(new, &old)| (old, new))
            .collect();

        let mut referenced: HashSet<usize> = HashSet::new();
        referenced.insert(0);
        let mut cards: Vec<TrailCard> = Vec::with_capacity(keep.len());
        for &old in &keep {
            let source = &self.cards[old];
            let mut needs: Vec<Need> = vec![];
            for need in &spliced[old] {
                let mut need = need.clone();
                need.reference = !referenced.insert(need.card);
                need.card = new_id[&need.card];
                needs.push(need);
            }
            cards.push(TrailCard {
                id: new_id[&old],
                depth: depth[&old],
                kind: source.kind,
                behavior: source.behavior.clone(),
                agent: source.agent.clone(),
                satisfied: source.satisfied,
                needs,
            });
        }

        Trail {
            agent: self.agent,
            behavior: self.behavior,
            satisfied: self.satisfied,
            cards,
        }
    }
}

/// One want in a contract, and whether it is held.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WantSummary {
    pub agent: String,
    pub behavior: String,
    pub satisfied: bool,
}

/// Every want in the contract, by agent then behavior.
pub fn wants(tracker: &Tracker) -> Vec<WantSummary> {
    let mut ret = vec![];
    for agent in tracker.get_working_agent_names() {
        let mut behaviors: Vec<String> =
            tracker.get_agent_wants(agent.clone()).into_iter().collect();
        behaviors.sort();
        for behavior in behaviors {
            let satisfied = tracker.resolve(&behavior).is_satisfied();
            ret.push(WantSummary {
                agent: agent.clone(),
                behavior,
                satisfied,
            });
        }
    }
    ret
}

struct TrailBuilder {
    cards: Vec<TrailCard>,
    placed: HashMap<(CardKind, String, Option<String>), usize>,
}

impl TrailBuilder {
    fn new() -> Self {
        TrailBuilder {
            cards: vec![],
            placed: HashMap::new(),
        }
    }

    /// The card for a promise, made at this depth if it is new. Returns the
    /// id and whether it was already there.
    fn place(
        &mut self,
        depth: usize,
        kind: CardKind,
        behavior: &str,
        agent: Option<&str>,
        satisfied: bool,
    ) -> (usize, bool) {
        let key = (kind, behavior.to_string(), agent.map(str::to_string));
        if let Some(&id) = self.placed.get(&key) {
            // The same provider can offer a behavior both outright and under
            // conditions; one held offer is the right reading.
            self.cards[id].satisfied |= satisfied;
            return (id, true);
        }
        let id = self.cards.len();
        self.cards.push(TrailCard {
            id,
            depth,
            kind,
            behavior: behavior.to_string(),
            agent: agent.map(str::to_string),
            satisfied,
            needs: vec![],
        });
        self.placed.insert(key, id);
        (id, false)
    }

    /// Hang a resolution under a card: one need per offer, or one need on a
    /// missing card when there are no offers.
    fn hang(&mut self, under: usize, depth: usize, resolution: &Resolution) {
        let behavior = resolution.get_behavior_name();
        let holder = self.cards[under].agent.clone();

        let offers: Vec<(&Offer, bool)> = resolution
            .get_satisfying_offers()
            .iter()
            .map(|o| (o, true))
            .chain(
                resolution
                    .get_unsatisfying_offers()
                    .iter()
                    .map(|o| (o, false)),
            )
            .collect();

        if offers.is_empty() {
            let (card, reference) = self.place(depth, CardKind::Missing, behavior, None, false);
            self.cards[under].needs.push(Need {
                behavior: behavior.to_string(),
                card,
                reference,
                own: false,
            });
            return;
        }

        for (offer, satisfied) in offers {
            let agent = offer.get_agent_name();
            let (card, reference) =
                self.place(depth, CardKind::Offer, behavior, Some(agent), satisfied);
            self.cards[under].needs.push(Need {
                behavior: behavior.to_string(),
                card,
                reference,
                own: holder.as_deref() == Some(agent),
            });
            if !reference {
                for condition in offer.get_resolved_conditions() {
                    self.hang(card, depth + 1, condition);
                }
            }
        }
    }
}

/// The trail for one agent's want.
///
/// The agent only names the want card; resolution is by behavior, so two
/// agents wanting the same thing get the same trail under different headers.
pub fn trail(tracker: &Tracker, agent: &str, want: &str) -> Trail {
    let resolution = tracker.resolve(want);
    let mut builder = TrailBuilder::new();
    let (root, _) = builder.place(
        0,
        CardKind::Want,
        want,
        Some(agent),
        resolution.is_satisfied(),
    );
    builder.hang(root, 1, &resolution);
    Trail {
        agent: agent.to_string(),
        behavior: want.to_string(),
        satisfied: resolution.is_satisfied(),
        cards: builder.cards,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Agent, Behavior};

    fn card<'a>(t: &'a Trail, behavior: &str, agent: Option<&str>) -> &'a TrailCard {
        t.cards
            .iter()
            .find(|c| c.behavior == behavior && c.agent.as_deref() == agent)
            .unwrap_or_else(|| panic!("no card {} by {:?} in {:#?}", behavior, agent, t))
    }

    #[test]
    fn simple_held_want() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));

        let t = trail(&tracker, "a1", "b1");
        assert!(t.satisfied);
        assert_eq!(t.width(), 2);
        assert_eq!(t.cards.len(), 2);
        assert_eq!(t.cards[0].kind, CardKind::Want);
        assert_eq!(t.cards[0].agent.as_deref(), Some("a1"));
        assert_eq!(
            t.cards[0].needs,
            vec![Need {
                behavior: "b1".into(),
                card: 1,
                reference: false,
                own: false
            }]
        );
        let offer = card(&t, "b1", Some("a2"));
        assert_eq!(offer.kind, CardKind::Offer);
        assert_eq!(offer.depth, 1);
        assert!(offer.satisfied);
        assert!(t.broken().is_empty());
    }

    #[test]
    fn missing_want() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));

        let t = trail(&tracker, "a1", "b1");
        assert!(!t.satisfied);
        let missing = card(&t, "b1", None);
        assert_eq!(missing.kind, CardKind::Missing);
        assert_eq!(missing.depth, 1);
        assert!(!missing.satisfied);
        assert_eq!(t.broken().len(), 1);
    }

    #[test]
    fn shared_condition_is_placed_once() {
        // b1 by a2 needs b2 and b3, both by a3, both resting on b4 by a4.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into(), "b3".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b2").with_conditions(vec!["b4".into()]),
            Behavior::build("b3").with_conditions(vec!["b4".into()]),
        ]));
        tracker.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b4")]));

        let t = trail(&tracker, "a1", "b1");
        assert!(t.satisfied);
        assert_eq!(t.width(), 4);
        // want, b1, b2, b4, b3: b4 is not repeated under b3.
        assert_eq!(t.cards.len(), 5);
        let b4 = card(&t, "b4", Some("a4"));
        assert_eq!(b4.depth, 3);

        let b2 = card(&t, "b2", Some("a3"));
        let b3 = card(&t, "b3", Some("a3"));
        assert_eq!(b2.depth, 2);
        assert_eq!(b3.depth, 2);
        assert_eq!(b2.needs.len(), 1);
        assert_eq!(b3.needs.len(), 1);
        assert_eq!(b2.needs[0].card, b4.id);
        assert_eq!(b3.needs[0].card, b4.id);
        // Whichever of b2 and b3 came first placed the card; the other refers.
        assert_ne!(b2.needs[0].reference, b3.needs[0].reference);
        assert_eq!(t.column(2).len(), 2);
    }

    #[test]
    fn broken_condition_marks_the_offer() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into()]),
        ]));

        let t = trail(&tracker, "a1", "b1");
        assert!(!t.satisfied);
        assert!(!card(&t, "b1", Some("a2")).satisfied);
        assert_eq!(card(&t, "b2", None).kind, CardKind::Missing);
        assert_eq!(t.broken().len(), 2);
    }

    #[test]
    fn own_condition_is_marked_and_foldable() {
        // a2 keeps b1 on b2, which a2 also promises, resting on b3 by a3.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into()]),
            Behavior::build("b2").with_conditions(vec!["b3".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b3")]));

        let t = trail(&tracker, "a1", "b1");
        let b1 = card(&t, "b1", Some("a2"));
        assert!(b1.needs[0].own);
        assert!(!t.cards[0].needs[0].own);
        assert_eq!(card(&t, "b3", Some("a3")).depth, 3);

        let folded = t.clone().without_own_promises();
        assert_eq!(folded.cards.len(), 3);
        assert!(folded.cards.iter().all(|c| c.behavior != "b2"));
        let b1 = card(&folded, "b1", Some("a2"));
        assert_eq!(b1.needs.len(), 1);
        assert_eq!(b1.needs[0].behavior, "b3");
        assert!(!b1.needs[0].own);
        let b3 = card(&folded, "b3", Some("a3"));
        assert_eq!(b3.depth, 2);
        assert_eq!(b1.needs[0].card, b3.id);
        assert_eq!(folded.width(), 3);
        // Ids are the positions again.
        for (i, c) in folded.cards.iter().enumerate() {
            assert_eq!(c.id, i);
        }
    }

    #[test]
    fn folding_a_trail_without_own_promises_changes_nothing() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b2")]));

        let t = trail(&tracker, "a1", "b1");
        assert_eq!(t.clone().without_own_promises(), t);
    }

    #[test]
    fn self_fulfilled_want_folds_to_the_want_alone() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("a1")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![Behavior::build("b1")]),
        );

        let t = trail(&tracker, "a1", "b1");
        assert!(t.cards[0].needs[0].own);
        let folded = t.without_own_promises();
        assert_eq!(folded.cards.len(), 1);
        assert!(folded.cards[0].needs.is_empty());
        assert!(folded.satisfied);
    }

    #[test]
    fn wants_lists_every_want_with_status() {
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("a1").with_wants(vec![Behavior::build("b2"), Behavior::build("b1")]),
        );
        tracker.add_agent(
            Agent::build("a2")
                .with_wants(vec![Behavior::build("b1")])
                .with_provides(vec![Behavior::build("b1")]),
        );

        assert_eq!(
            wants(&tracker),
            vec![
                WantSummary {
                    agent: "a1".into(),
                    behavior: "b1".into(),
                    satisfied: true
                },
                WantSummary {
                    agent: "a1".into(),
                    behavior: "b2".into(),
                    satisfied: false
                },
                WantSummary {
                    agent: "a2".into(),
                    behavior: "b1".into(),
                    satisfied: true
                },
            ]
        );
    }

    #[test]
    fn empty_tracker() {
        let tracker = Tracker::new();
        assert!(wants(&tracker).is_empty());
        let t = trail(&tracker, "nobody", "nothing");
        assert!(!t.satisfied);
        assert_eq!(t.cards.len(), 2);
    }
}
