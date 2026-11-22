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

use crate::components::{mark_spans, merge_spans};
use crate::marks::Marks;
use crate::resolve::{Offer, Resolution, Spans};
use crate::Tracker;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

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
    /// The promises a bundled card stands for, in trail order; see
    /// [`Trail::bundled`]. Empty on a card that stands for one promise.
    #[serde(default)]
    pub promises: Vec<BundledPromise>,
}

/// One promise inside a bundled card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledPromise {
    pub behavior: String,
    pub satisfied: bool,
}

impl TrailCard {
    /// How many promises the card stands for: one, or a bundle's count.
    pub fn count(&self) -> usize {
        self.promises.len().max(1)
    }

    /// How many of those are not held.
    pub fn unsatisfied(&self) -> usize {
        if self.promises.is_empty() {
            usize::from(!self.satisfied)
        } else {
            self.promises.iter().filter(|p| !p.satisfied).count()
        }
    }

    /// The behaviors the card stands for.
    pub fn behaviors(&self) -> Vec<&str> {
        if self.promises.is_empty() {
            vec![self.behavior.as_str()]
        } else {
            self.promises.iter().map(|p| p.behavior.as_str()).collect()
        }
    }
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
    /// Where substituted values sit in the names on this trail, for
    /// [`Trail::labels`]. Display only: the cards keep plain names. Not
    /// serialized, so a trail read back is labelled plainly.
    #[serde(skip)]
    spans: TrailSpans,
}

/// What the holder of one need filled in, by behavior needed: a value its
/// promise's goal bound, its own name, one a sibling condition found, or
/// what its `{{?x}}` matched. One behavior for a plain need; a bundled need
/// names several. A behavior with nothing filled in has no entry.
type NeedSpans = BTreeMap<String, Spans>;

/// The spans for a need of `behavior`.
fn need_spans(behavior: &str, spans: &[(usize, usize)]) -> NeedSpans {
    let mut held = NeedSpans::new();
    if !spans.is_empty() {
        held.insert(behavior.to_string(), spans.to_vec());
    }
    held
}

/// Add what `from` filled in to `into`, behavior by behavior.
fn merge_need_spans(into: &mut NeedSpans, from: &NeedSpans) {
    for (behavior, spans) in from {
        let held = into.entry(behavior.clone()).or_default();
        *held = merge_spans(held, spans);
    }
}

/// The substituted parts of the names on one trail, as this trail reached
/// them. The same text reached by another want, or written out by another
/// promise that is not on the trail, has no say here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TrailSpans {
    /// The want, as its agent wrote it.
    want: Spans,
    /// One entry per need row, in step with [`Trail::cards`] and each
    /// card's [`TrailCard::needs`]. Kept by row, not by what the need is: a
    /// card that takes over another's needs when self promises are folded
    /// can show the same behavior from the same provider twice, once as its
    /// own promise wrote it and once as the folded one reached it, and each
    /// row says only what its own condition filled in. Rows that are merged
    /// into one (bundling) are merged here too.
    needs: Vec<Vec<NeedSpans>>,
    /// (agent, behavior) — what the promise keeping the behavior names
    /// through a variable.
    offers: BTreeMap<(String, String), Spans>,
}

impl TrailSpans {
    fn is_empty(&self) -> bool {
        self.want.is_empty()
            && self.offers.is_empty()
            && self.needs.iter().flatten().all(BTreeMap::is_empty)
    }

    fn add_offer(&mut self, agent: &str, behavior: &str, spans: &[(usize, usize)]) {
        if spans.is_empty() {
            return;
        }
        let held = self
            .offers
            .entry((agent.to_string(), behavior.to_string()))
            .or_default();
        *held = merge_spans(held, spans);
    }

    /// Everything need row `row` of card `card` filled in. Empty for a
    /// trail that carries no spans (one read back from JSON).
    fn row(&self, card: usize, row: usize) -> NeedSpans {
        self.needs
            .get(card)
            .and_then(|rows| rows.get(row))
            .cloned()
            .unwrap_or_default()
    }

    /// What need row `row` of card `card` filled in on `behavior`.
    fn need(&self, card: usize, row: usize, behavior: &str) -> &[(usize, usize)] {
        self.needs
            .get(card)
            .and_then(|rows| rows.get(row))
            .and_then(|held| held.get(behavior))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// What the promise by which `card`'s agent keeps `behavior` names
    /// through a variable.
    fn offer(&self, card: &TrailCard, behavior: &str) -> &[(usize, usize)] {
        match (card.kind, &card.agent) {
            (CardKind::Offer, Some(agent)) => self
                .offers
                .get(&(agent.clone(), behavior.to_string()))
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            _ => &[],
        }
    }
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

    /// Promises on the trail, past the want, counting inside bundles.
    pub fn promise_count(&self) -> usize {
        self.cards
            .iter()
            .filter(|c| c.kind != CardKind::Want)
            .map(TrailCard::count)
            .sum()
    }

    /// Promises on the trail that are not held, counting inside bundles.
    pub fn broken_count(&self) -> usize {
        self.cards
            .iter()
            .filter(|c| c.kind != CardKind::Want)
            .map(TrailCard::unsatisfied)
            .sum()
    }

    /// The trail with the promises one provider makes in one column merged
    /// into a single card.
    ///
    /// The merged card lists its promises in [`TrailCard::promises`], names
    /// them all in its behavior, and is held only when every one of them is:
    /// a broken promise inside keeps the bundle broken. Its needs are the
    /// union of its members', and two needs from one card that now land on
    /// the same bundle become one need naming both behaviors. Want and missing
    /// cards are never merged. Columns do not move, references are re-marked
    /// in card order, and ids are positions again.
    pub fn bundled(self) -> Trail {
        // Which bundle each card falls in, bundles in order of first card.
        let mut bundle_of: Vec<usize> = Vec::with_capacity(self.cards.len());
        let mut bundles: Vec<Vec<usize>> = vec![];
        let mut by_key: HashMap<(usize, &str), usize> = HashMap::new();
        for card in &self.cards {
            let key = match (card.kind, card.agent.as_deref()) {
                (CardKind::Offer, Some(agent)) => Some((card.depth, agent)),
                _ => None,
            };
            let at = match key.and_then(|k| by_key.get(&k).copied()) {
                Some(at) => at,
                None => {
                    if let Some(k) = key {
                        by_key.insert(k, bundles.len());
                    }
                    bundles.push(vec![]);
                    bundles.len() - 1
                }
            };
            bundles[at].push(card.id);
            bundle_of.push(at);
        }
        if bundles.len() == self.cards.len() {
            return self;
        }

        let mut referenced: HashSet<usize> = HashSet::new();
        referenced.insert(0);
        let mut cards: Vec<TrailCard> = Vec::with_capacity(bundles.len());
        let mut need_spans: Vec<Vec<NeedSpans>> = Vec::with_capacity(bundles.len());
        for (id, members) in bundles.iter().enumerate() {
            let first = &self.cards[members[0]];

            // Needs in member order, one per bundle reached.
            // The rows merged into one need are one row for the spans too:
            // what any of them filled in, behavior by behavior.
            let mut reached: Vec<(usize, bool, Vec<&str>, NeedSpans)> = vec![];
            for &m in members {
                for (row, need) in self.cards[m].needs.iter().enumerate() {
                    let target = bundle_of[need.card];
                    if target == id {
                        continue;
                    }
                    let filled = self.spans.row(m, row);
                    match reached.iter_mut().find(|(card, ..)| *card == target) {
                        Some((_, _, behaviors, held)) => {
                            if !behaviors.contains(&need.behavior.as_str()) {
                                behaviors.push(&need.behavior);
                            }
                            merge_need_spans(held, &filled);
                        }
                        None => reached.push((target, need.own, vec![&need.behavior], filled)),
                    }
                }
            }
            let mut rows: Vec<NeedSpans> = Vec::with_capacity(reached.len());
            let needs: Vec<Need> = reached
                .into_iter()
                .map(|(card, own, behaviors, filled)| {
                    rows.push(filled);
                    Need {
                        behavior: behaviors.join(", "),
                        card,
                        reference: !referenced.insert(card),
                        own,
                    }
                })
                .collect();
            need_spans.push(rows);

            let mut card = TrailCard {
                id,
                depth: first.depth,
                kind: first.kind,
                behavior: first.behavior.clone(),
                agent: first.agent.clone(),
                satisfied: first.satisfied,
                needs,
                promises: vec![],
            };
            if members.len() > 1 {
                card.promises = members
                    .iter()
                    .map(|&m| BundledPromise {
                        behavior: self.cards[m].behavior.clone(),
                        satisfied: self.cards[m].satisfied,
                    })
                    .collect();
                card.satisfied = card.promises.iter().all(|p| p.satisfied);
                card.behavior = card
                    .promises
                    .iter()
                    .map(|p| p.behavior.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            cards.push(card);
        }

        // A promise's own spans are kept by name, and a bundle still answers
        // to each of its promises' names.
        Trail {
            agent: self.agent,
            behavior: self.behavior,
            satisfied: self.satisfied,
            cards,
            spans: TrailSpans {
                want: self.spans.want,
                needs: need_spans,
                offers: self.spans.offers,
            },
        }
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
        //
        // A need that moves under a new holder is a row of its own there,
        // even beside a need of the holder's for the same behavior from the
        // same provider, and takes along what the promise it hung from
        // filled in: each row keeps its own spans.
        let mut spliced: Vec<Vec<(Need, NeedSpans)>> = Vec::with_capacity(self.cards.len());
        for (id, card) in self.cards.iter().enumerate() {
            let mut out: Vec<(Need, NeedSpans)> = vec![];
            let mut seen: HashSet<usize> = HashSet::new();
            let mut queue: VecDeque<(usize, usize)> =
                (0..card.needs.len()).map(|row| (id, row)).collect();
            while let Some((from, row)) = queue.pop_front() {
                let need = &self.cards[from].needs[row];
                if need.own {
                    if seen.insert(need.card) {
                        queue.extend(
                            (0..self.cards[need.card].needs.len()).map(|row| (need.card, row)),
                        );
                    }
                } else {
                    out.push((need.clone(), self.spans.row(from, row)));
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
            for (need, _) in &spliced[id] {
                if let std::collections::hash_map::Entry::Vacant(e) = depth.entry(need.card) {
                    e.insert(d + 1);
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
        let mut need_spans: Vec<Vec<NeedSpans>> = Vec::with_capacity(keep.len());
        for &old in &keep {
            let source = &self.cards[old];
            let mut needs: Vec<Need> = vec![];
            let mut rows: Vec<NeedSpans> = vec![];
            for (need, filled) in &spliced[old] {
                let mut need = need.clone();
                need.reference = !referenced.insert(need.card);
                need.card = new_id[&need.card];
                needs.push(need);
                rows.push(filled.clone());
            }
            need_spans.push(rows);
            cards.push(TrailCard {
                id: new_id[&old],
                depth: depth[&old],
                kind: source.kind,
                behavior: source.behavior.clone(),
                agent: source.agent.clone(),
                satisfied: source.satisfied,
                needs,
                promises: source.promises.clone(),
            });
        }

        Trail {
            agent: self.agent,
            behavior: self.behavior,
            satisfied: self.satisfied,
            cards,
            spans: TrailSpans {
                want: self.spans.want,
                needs: need_spans,
                offers: self.spans.offers,
            },
        }
    }
}

impl Trail {
    /// [`Trail::labels_when`], marking unless `marks` is empty: the option
    /// is off, or the contract substitutes nothing anywhere.
    pub fn labels(&self, marks: &Marks) -> Vec<CardLabels> {
        self.labels_when(!marks.is_empty())
    }
}

/// What one card says when substituted values are marked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardLabels {
    /// In place of [`TrailCard::behavior`].
    pub behavior: String,
    /// In place of each of [`TrailCard::promises`], in order.
    pub promises: Vec<String>,
    /// In place of each of [`TrailCard::needs`], in order.
    pub needs: Vec<String>,
}

/// A bundled need names several behaviors in one string. Take it apart
/// again, as the behaviors of the card it points at. None when it is not
/// made of those.
fn split_joined<'a>(joined: &str, behaviors: &[&'a str]) -> Option<Vec<&'a str>> {
    for behavior in behaviors {
        let Some(rest) = joined.strip_prefix(behavior) else {
            continue;
        };
        if rest.is_empty() {
            return Some(vec![behavior]);
        }
        if let Some(mut tail) = rest
            .strip_prefix(", ")
            .and_then(|rest| split_joined(rest, behaviors))
        {
            tail.insert(0, behavior);
            return Some(tail);
        }
    }
    None
}

impl Trail {
    /// What every card says with substituted values marked, by card id.
    ///
    /// Read off the trail as it is about to be shown — after bundling and
    /// self folding — so the cards themselves keep plain names for
    /// selection, comments and every other lookup.
    ///
    /// The marks are this trail's own: what resolution substituted on the
    /// way down from this want, and nothing another want or another promise
    /// did with the same text. With `mark` false (the option off) every
    /// label is the plain name; the trail needs nothing else to mark itself,
    /// so a view has no whole-tracker [`Marks`] to build for it.
    ///
    /// - The want card: as its agent wrote the want.
    /// - A need: what the condition it stands for filled in, together with
    ///   what the promise answering it names through a variable. Two rows on
    ///   one card for the same behavior — the card's own condition, and one
    ///   taken over from a folded self promise — are each their own.
    /// - A promise card: what its own promise names through a variable,
    ///   together with what every need that reaches it on this trail filled
    ///   in. A card placed once for several needs shows all of theirs.
    /// - A missing card: what the needs that reach it filled in.
    ///
    /// A bundled card or need stands for several promises and shows what any
    /// of those substituted, each behavior by itself.
    pub fn labels_when(&self, mark: bool) -> Vec<CardLabels> {
        if !mark || self.spans.is_empty() {
            return self
                .cards
                .iter()
                .map(|card| CardLabels {
                    behavior: card.behavior.clone(),
                    promises: card.promises.iter().map(|p| p.behavior.clone()).collect(),
                    needs: card.needs.iter().map(|n| n.behavior.clone()).collect(),
                })
                .collect();
        }

        // The needs first: each says what its holder filled in, and that is
        // also part of what the card it reaches shows.
        let mut reached: HashMap<(usize, &str), Spans> = HashMap::new();
        let mut needs: Vec<Vec<String>> = Vec::with_capacity(self.cards.len());
        for (at, card) in self.cards.iter().enumerate() {
            let mut labels = Vec::with_capacity(card.needs.len());
            for (row, need) in card.needs.iter().enumerate() {
                let Some(target) = self.cards.get(need.card) else {
                    labels.push(need.behavior.clone());
                    continue;
                };
                let Some(parts) = split_joined(&need.behavior, &target.behaviors()) else {
                    labels.push(need.behavior.clone());
                    continue;
                };
                let parts: Vec<String> = parts
                    .into_iter()
                    .map(|part| {
                        let filled = self.spans.need(at, row, part);
                        let label =
                            mark_spans(part, &merge_spans(filled, self.spans.offer(target, part)));
                        let held = reached.entry((need.card, part)).or_default();
                        *held = merge_spans(held, filled);
                        label
                    })
                    .collect();
                labels.push(parts.join(", "));
            }
            needs.push(labels);
        }

        self.cards
            .iter()
            .zip(needs)
            .enumerate()
            .map(|(at, (card, needs))| {
                let label = |behavior: &str| -> String {
                    if card.kind == CardKind::Want {
                        return mark_spans(behavior, &self.spans.want);
                    }
                    let own = self.spans.offer(card, behavior);
                    match reached.get(&(at, behavior)) {
                        Some(filled) => mark_spans(behavior, &merge_spans(filled, own)),
                        None => mark_spans(behavior, own),
                    }
                };
                let promises: Vec<String> =
                    card.promises.iter().map(|p| label(&p.behavior)).collect();
                let behavior = if promises.is_empty() {
                    label(&card.behavior)
                } else {
                    promises.join(", ")
                };
                CardLabels {
                    behavior,
                    promises,
                    needs,
                }
            })
            .collect()
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
    spans: TrailSpans,
}

impl TrailBuilder {
    fn new() -> Self {
        TrailBuilder {
            cards: vec![],
            placed: HashMap::new(),
            spans: TrailSpans::default(),
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
        self.spans.needs.push(vec![]);
        self.cards.push(TrailCard {
            id,
            depth,
            kind,
            behavior: behavior.to_string(),
            agent: agent.map(str::to_string),
            satisfied,
            needs: vec![],
            promises: vec![],
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
            self.spans.needs[under].push(need_spans(behavior, resolution.get_bound()));
            self.cards[under].needs.push(Need {
                behavior: behavior.to_string(),
                card,
                reference,
                own: false,
            });
            return;
        }

        for (offer, satisfied) in offers {
            // An offer answering an open goal is carded under the concrete
            // behavior it keeps, and carries what the goal filled in or
            // matched there itself.
            let filled = match offer.get_behavior() {
                Some(_) => offer.get_matched(),
                None => resolution.get_bound(),
            };
            let behavior = offer.get_behavior().unwrap_or(behavior);
            let agent = offer.get_agent_name();
            let filled = need_spans(behavior, filled);
            self.spans.add_offer(agent, behavior, offer.get_bound());
            let (card, reference) =
                self.place(depth, CardKind::Offer, behavior, Some(agent), satisfied);
            self.cards[under].needs.push(Need {
                behavior: behavior.to_string(),
                card,
                reference,
                own: holder.as_deref() == Some(agent),
            });
            self.spans.needs[under].push(filled);
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
/// The agent names the want card and says how the want was written
/// ([`Tracker::resolve_want`]); resolution is by behavior, so two agents
/// wanting the same thing get the same cards under different headers.
pub fn trail(tracker: &Tracker, agent: &str, want: &str) -> Trail {
    trail_with(tracker, agent, want, false)
}

/// [`trail`], with the "Fold: Unneeded alternatives" display option: when
/// `fold_alternatives` is on, offers that do not keep a behavior another
/// offer already keeps are left out, with everything they rest on
/// ([`Resolution::without_unneeded_alternatives`]).
pub fn trail_with(tracker: &Tracker, agent: &str, want: &str, fold_alternatives: bool) -> Trail {
    let mut resolution = tracker.resolve_want(agent, want);
    if fold_alternatives {
        resolution = resolution.without_unneeded_alternatives();
    }
    let mut builder = TrailBuilder::new();
    builder.spans.want = resolution.get_bound().to_vec();
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
        spans: builder.spans,
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

    /// a2 keeps b1 on b2, b3 and b4, all from a3; b3 and b4 rest on b5 by
    /// a4, and b4 also on b6, which nobody promises.
    fn one_provider_three_promises() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into(), "b3".into(), "b4".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b2"),
            Behavior::build("b3").with_conditions(vec!["b5".into()]),
            Behavior::build("b4").with_conditions(vec!["b5".into(), "b6".into()]),
        ]));
        tracker.add_agent(Agent::build("a4").with_provides(vec![Behavior::build("b5")]));
        tracker
    }

    #[test]
    fn bundling_merges_one_providers_cards_in_a_column() {
        let t = trail(&one_provider_three_promises(), "a1", "b1");
        assert_eq!(t.cards.len(), 7);
        let b = t.clone().bundled();

        // want, b1, the a3 bundle, b5, b6
        assert_eq!(b.cards.len(), 5);
        for (i, c) in b.cards.iter().enumerate() {
            assert_eq!(c.id, i);
        }
        let bundle = card(&b, "b2, b3, b4", Some("a3"));
        assert_eq!(bundle.depth, 2);
        assert_eq!(bundle.count(), 3);
        assert_eq!(bundle.behaviors(), vec!["b2", "b3", "b4"]);
        // b4 is broken, so the bundle is too, and says which one.
        assert!(!bundle.satisfied);
        assert_eq!(bundle.unsatisfied(), 1);
        assert_eq!(
            bundle.promises[2],
            BundledPromise {
                behavior: "b4".into(),
                satisfied: false
            }
        );

        // b1 had three needs into a3; now one, naming all three.
        let b1 = card(&b, "b1", Some("a2"));
        assert_eq!(b1.needs.len(), 1);
        assert_eq!(b1.needs[0].behavior, "b2, b3, b4");
        assert_eq!(b1.needs[0].card, bundle.id);
        assert!(!b1.needs[0].reference);

        // b5 was needed by b3 and by b4; the bundle reaches it once.
        let b5 = card(&b, "b5", Some("a4"));
        let b6 = card(&b, "b6", None);
        let targets: Vec<usize> = bundle.needs.iter().map(|n| n.card).collect();
        assert_eq!(targets, vec![b5.id, b6.id]);
        assert!(bundle.needs.iter().all(|n| !n.reference));

        // Counts see inside the bundle.
        assert_eq!(b.promise_count(), t.promise_count());
        assert_eq!(b.promise_count(), 6);
        assert_eq!(b.broken_count(), t.broken_count());
        assert_eq!(b.broken().len(), 3);
    }

    #[test]
    fn bundling_a_trail_with_nothing_to_merge_changes_nothing() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b2".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("b2")]));

        let t = trail(&tracker, "a1", "b1");
        assert_eq!(t.clone().bundled(), t);
    }

    #[test]
    fn bundling_keeps_columns_apart_and_missing_cards_alone() {
        // a2 promises b1 and, a column further on, b3; b2 and b4 are missing.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["b5".into()]),
            Behavior::build("b3").with_conditions(vec!["b4".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("b5").with_conditions(vec!["b3".into(), "b2".into()]),
        ]));

        let t = trail(&tracker, "a1", "b1");
        let b = t.clone().bundled();
        assert_eq!(b, t);
        assert!(b.cards.iter().all(|c| c.promises.is_empty()));
    }

    #[test]
    fn bundling_after_folding_own_promises() {
        let t = trail(&one_provider_three_promises(), "a1", "b1")
            .without_own_promises()
            .bundled();
        assert_eq!(t.cards.len(), 5);
        assert_eq!(card(&t, "b2, b3, b4", Some("a3")).count(), 3);
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

    #[test]
    fn labels_mark_each_card_and_each_need() {
        let tracker = crate::marks::tracker(crate::marks::IAM);
        let marks = Marks::new(&tracker);
        let want = "Read Secret Contents appsecret by appname";
        let t = trail(&tracker, "appname", want);
        let labels = t.labels(&marks);
        assert_eq!(labels.len(), t.cards.len());

        let label = |behavior: &str, agent: &str| -> &CardLabels {
            &labels[card(&t, behavior, Some(agent)).id]
        };
        // The want as appname wrote it; the need under it as AWSSM keeps it.
        assert_eq!(
            labels[0].behavior,
            "Read Secret Contents appsecret by <appname>"
        );
        assert_eq!(
            labels[0].needs,
            vec!["Read Secret Contents <appsecret> by <appname>"]
        );
        let awssm = label(want, "AWSSM");
        assert_eq!(
            awssm.behavior,
            "Read Secret Contents <appsecret> by <appname>"
        );
        assert_eq!(
            awssm.needs,
            vec![
                "<appname> has IAM role <approle>",
                "IAM access granted for reading Secret <appsecret> to <approle>",
            ]
        );
        assert_eq!(
            label("appname has IAM role approle", "appname").behavior,
            "<appname> has IAM role <approle>"
        );
        assert_eq!(
            label(
                "Design approval access to Secret appsecret for appname",
                "manager"
            )
            .behavior,
            "Design approval access to Secret <appsecret> for <appname>"
        );
        // The cards themselves still hold plain names.
        assert!(t.cards.iter().all(|c| !c.behavior.contains('<')));
    }

    #[test]
    fn labels_follow_a_bundled_and_self_folded_trail() {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec![
                "{{!self}} is fed".to_string(),
                "{{!self}} is up, mostly".to_string(),
                "{{!self}} is nowhere".to_string(),
            ]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("a2 is fed"),
            Behavior::build("a2 is up, mostly"),
        ]));
        let marks = Marks::new(&tracker);

        let t = trail(&tracker, "a1", "b1").bundled();
        let labels = t.labels(&marks);
        let a3 = t
            .cards
            .iter()
            .find(|c| c.agent.as_deref() == Some("a3"))
            .unwrap();
        assert_eq!(a3.behavior, "a2 is fed, a2 is up, mostly");
        assert_eq!(labels[a3.id].behavior, "<a2> is fed, <a2> is up, mostly");
        assert_eq!(
            labels[a3.id].promises,
            vec!["<a2> is fed", "<a2> is up, mostly"]
        );
        let a2 = card(&t, "b1", Some("a2"));
        assert_eq!(
            labels[a2.id].needs,
            vec!["<a2> is fed, <a2> is up, mostly", "<a2> is nowhere"]
        );
        let missing = card(&t, "a2 is nowhere", None);
        assert_eq!(labels[missing.id].behavior, "<a2> is nowhere");

        // With nothing substituted the labels are the names.
        let mut plain = Tracker::new();
        plain.add_agent(Agent::build("a1").with_wants(vec![Behavior::build("b1")]));
        plain.add_agent(Agent::build("a2").with_provides(vec![Behavior::build("b1")]));
        let t = trail(&plain, "a1", "b1");
        for (card, labels) in t.cards.iter().zip(t.labels(&Marks::new(&plain))) {
            assert_eq!(labels.behavior, card.behavior);
            assert_eq!(
                labels.needs,
                card.needs
                    .iter()
                    .map(|n| n.behavior.clone())
                    .collect::<Vec<_>>()
            );
        }
    }

    /// a1 keeps mid1 on `a2 is up` written out; a2 reaches the same text as
    /// `{{!self}} is up`, in a want and in a condition; a3 promises it.
    fn written_out_and_through_a_variable() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("a0").with_provides(vec![
            Behavior::build("top").with_conditions(vec!["mid1".into(), "mid2".into()]),
        ]));
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("mid1").with_conditions(vec!["a2 is up".into()]),
        ]));
        tracker.add_agent(
            Agent::build("a2")
                .with_wants(vec![Behavior::build("{{!self}} is up")])
                .with_provides(vec![
                    Behavior::build("mid2").with_conditions(vec!["{{!self}} is up".into()])
                ]),
        );
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("a2 is up")]));
        tracker.add_agent(
            Agent::build("w").with_wants(vec![Behavior::build("mid1"), Behavior::build("top")]),
        );
        tracker
    }

    #[test]
    fn labels_are_the_trails_own_not_another_wants() {
        let tracker = written_out_and_through_a_variable();
        let marks = Marks::new(&tracker);
        assert!(!marks.is_empty());

        // From w's want, a1 names the behavior outright and a3 promises it
        // outright: nothing on this trail was substituted, whatever a2 does
        // with the same text elsewhere.
        let t = trail(&tracker, "w", "mid1");
        let labels = t.labels(&marks);
        let a1 = card(&t, "mid1", Some("a1"));
        let a3 = card(&t, "a2 is up", Some("a3"));
        assert_eq!(labels[a1.id].needs, vec!["a2 is up"]);
        assert_eq!(labels[a3.id].behavior, "a2 is up");
        for (card, said) in t.cards.iter().zip(&labels) {
            assert_eq!(said.behavior, card.behavior);
        }

        // From a2's own want, it is a2's name all the way.
        let t = trail(&tracker, "a2", "a2 is up");
        let labels = t.labels(&marks);
        assert_eq!(labels[0].behavior, "<a2> is up");
        assert_eq!(labels[0].needs, vec!["<a2> is up"]);
        assert_eq!(
            labels[card(&t, "a2 is up", Some("a3")).id].behavior,
            "<a2> is up"
        );

        // Asked for the same text by an agent that does not want it that
        // way, the want card is plain too.
        let t = trail(&tracker, "w", "a2 is up");
        assert!(t.labels(&marks).iter().all(|l| !l.behavior.contains('<')));
    }

    #[test]
    fn a_card_reached_by_several_needs_shows_what_each_filled_in() {
        let tracker = written_out_and_through_a_variable();
        let marks = Marks::new(&tracker);
        let t = trail(&tracker, "w", "top");
        let labels = t.labels(&marks);
        // Each need as its own holder has it ...
        assert_eq!(
            labels[card(&t, "mid1", Some("a1")).id].needs,
            vec!["a2 is up"]
        );
        assert_eq!(
            labels[card(&t, "mid2", Some("a2")).id].needs,
            vec!["<a2> is up"]
        );
        // ... and the one card both reach shows the two together.
        assert_eq!(
            labels[card(&t, "a2 is up", Some("a3")).id].behavior,
            "<a2> is up"
        );
    }

    #[test]
    fn labels_are_plain_when_marking_is_off() {
        let tracker = crate::marks::tracker(crate::marks::IAM);
        let t = trail(&tracker, "appname", "Read Secret Contents appsecret by appname");
        for shown in [t.clone(), t.clone().bundled(), t.without_own_promises()] {
            let labels = shown.labels(&Marks::default());
            assert_eq!(labels.len(), shown.cards.len());
            for (card, said) in shown.cards.iter().zip(labels) {
                assert_eq!(said.behavior, card.behavior);
                assert_eq!(
                    said.promises,
                    card.promises
                        .iter()
                        .map(|p| p.behavior.clone())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    said.needs,
                    card.needs
                        .iter()
                        .map(|n| n.behavior.clone())
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn a_need_folded_under_another_card_keeps_its_marks() {
        // a2 keeps b1 on a promise of its own, which rests on a3 by a2's
        // own name; a1's b0 names the same behavior outright.
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("w").with_wants(vec![Behavior::build("b1"), Behavior::build("b0")]),
        );
        tracker.add_agent(Agent::build("a1").with_provides(vec![
            Behavior::build("b0").with_conditions(vec!["a2 is fed".into()]),
        ]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["inner".into()]),
            Behavior::build("inner").with_conditions(vec!["{{!self}} is fed".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![Behavior::build("a2 is fed")]));
        let marks = Marks::new(&tracker);

        let full = trail(&tracker, "w", "b1");
        let labels = full.labels(&marks);
        assert_eq!(
            labels[card(&full, "inner", Some("a2")).id].needs,
            vec!["<a2> is fed"]
        );
        assert_eq!(labels[card(&full, "b1", Some("a2")).id].needs, vec!["inner"]);

        for t in [
            full.clone().without_own_promises(),
            full.clone().without_own_promises().bundled(),
            full.clone().bundled().without_own_promises(),
        ] {
            let labels = t.labels(&marks);
            let b1 = card(&t, "b1", Some("a2"));
            assert_eq!(b1.needs.len(), 1);
            assert_eq!(b1.needs[0].behavior, "a2 is fed");
            assert_eq!(labels[b1.id].needs, vec!["<a2> is fed"]);
            assert_eq!(
                labels[card(&t, "a2 is fed", Some("a3")).id].behavior,
                "<a2> is fed"
            );
        }

        // Folding does not hand the marks to a holder that never had them.
        let other = trail(&tracker, "w", "b0").without_own_promises();
        let labels = other.labels(&marks);
        assert_eq!(
            labels[card(&other, "b0", Some("a1")).id].needs,
            vec!["a2 is fed"]
        );
        assert_eq!(
            labels[card(&other, "a2 is fed", Some("a3")).id].behavior,
            "a2 is fed"
        );
    }

    /// a keeps `top` on `z is up` written out and on a promise of its own,
    /// `inner`, whose open condition matches the same behavior.
    const FOLDED_BESIDE_WRITTEN_OUT: &str = r#"
kind: Agent
name: w
wants: [{name: top}]
---
kind: Agent
name: a
provides:
- name: top
  conditions: ["z is up", "inner"]
- name: inner
  conditions: ["{{?v}} is up"]
---
kind: Agent
name: z
provides: [{name: z is up}]
"#;

    #[test]
    fn a_need_folded_in_beside_the_holders_own_keeps_to_its_own_row() {
        let tracker = crate::marks::tracker(FOLDED_BESIDE_WRITTEN_OUT);
        let marks = Marks::new(&tracker);
        let full = trail(&tracker, "w", "top");

        // Unfolded: each card has its own need.
        let labels = full.labels(&marks);
        assert_eq!(
            labels[card(&full, "top", Some("a")).id].needs,
            vec!["z is up", "inner"]
        );
        assert_eq!(
            labels[card(&full, "inner", Some("a")).id].needs,
            vec!["<z> is up"]
        );
        assert_eq!(
            labels[card(&full, "z is up", Some("z")).id].behavior,
            "<z> is up"
        );

        // Folded: `top` shows both needs, as two rows for the same behavior
        // from the same provider. Only the one taken over from `inner` was
        // found through a variable.
        for t in [
            full.clone().without_own_promises(),
            full.clone().without_own_promises().bundled(),
            full.clone().bundled().without_own_promises(),
        ] {
            let labels = t.labels(&marks);
            let top = card(&t, "top", Some("a"));
            let rows: Vec<(&str, bool)> = top
                .needs
                .iter()
                .map(|n| (n.behavior.as_str(), n.reference))
                .collect();
            assert_eq!(rows, vec![("z is up", false), ("z is up", true)]);
            assert_eq!(labels[top.id].needs, vec!["z is up", "<z> is up"]);
            // The one card both rows reach shows what either filled in.
            assert_eq!(
                labels[card(&t, "z is up", Some("z")).id].behavior,
                "<z> is up"
            );
        }

        // Off, nothing is marked either way.
        let folded = full.without_own_promises();
        assert_eq!(folded.labels_when(true), folded.labels(&marks));
        assert_eq!(folded.labels_when(false), folded.labels(&Marks::default()));
        let labels = folded.labels_when(false);
        assert_eq!(
            labels[card(&folded, "top", Some("a")).id].needs,
            vec!["z is up", "z is up"]
        );
    }

    #[test]
    fn two_conditions_of_one_promise_reaching_one_behavior_are_each_their_own() {
        // One promise, the behavior written out and found again by `{{?v}}`.
        let tracker = crate::marks::tracker(
            r#"
kind: Agent
name: w
wants: [{name: top}]
---
kind: Agent
name: a
provides:
- name: top
  conditions: ["é is up", "{{?v}} is up"]
---
kind: Agent
name: "é"
provides: [{name: "é is up"}]
"#,
        );
        let marks = Marks::new(&tracker);
        let t = trail(&tracker, "w", "top");
        let labels = t.labels(&marks);
        assert_eq!(
            labels[card(&t, "top", Some("a")).id].needs,
            vec!["\u{e9} is up", "<\u{e9}> is up"]
        );
    }

    #[test]
    fn bundling_merges_a_written_out_need_with_one_found_through_a_variable() {
        // h's m1 and m2 sit in one column and both reach `z is up`, m1
        // written out and m2 through `{{?v}}`; g's m3 beside them names it
        // outright.
        let tracker = crate::marks::tracker(
            r#"
kind: Agent
name: w
wants: [{name: top}]
---
kind: Agent
name: t
provides:
- name: top
  conditions: [m1, m2, m3]
---
kind: Agent
name: h
provides:
- name: m1
  conditions: ["z is up"]
- name: m2
  conditions: ["{{?v}} is up"]
---
kind: Agent
name: g
provides:
- name: m3
  conditions: ["z is up"]
---
kind: Agent
name: z
provides: [{name: z is up}]
"#,
        );
        let marks = Marks::new(&tracker);
        let b = trail(&tracker, "w", "top").bundled();
        let labels = b.labels(&marks);
        let bundle = card(&b, "m1, m2", Some("h"));
        // One row for both of h's needs: what either filled in.
        assert_eq!(bundle.needs.len(), 1);
        assert_eq!(labels[bundle.id].needs, vec!["<z> is up"]);
        assert_eq!(labels[card(&b, "m3", Some("g")).id].needs, vec!["z is up"]);
    }

    #[test]
    fn a_bundle_shows_what_the_promises_it_merges_filled_in_and_no_others() {
        // a3 keeps x1 on an `... is up` written out and x2 on an open
        // condition that matches it; a4 keeps y1 on it written out.
        let mut tracker = Tracker::new();
        tracker.add_agent(Agent::build("w").with_wants(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("a2").with_provides(vec![
            Behavior::build("b1").with_conditions(vec!["x1".into(), "x2".into(), "y1".into()]),
        ]));
        tracker.add_agent(Agent::build("a3").with_provides(vec![
            Behavior::build("x1").with_conditions(vec!["\u{e9}9 is up".into()]),
            Behavior::build("x2").with_conditions(vec!["{{?who}} is up".into()]),
        ]));
        tracker.add_agent(Agent::build("a4").with_provides(vec![
            Behavior::build("y1").with_conditions(vec!["\u{e9}9 is up".into()]),
        ]));
        tracker
            .add_agent(Agent::build("\u{e9}9").with_provides(vec![Behavior::build("\u{e9}9 is up")]));
        let marks = Marks::new(&tracker);

        let t = trail(&tracker, "w", "b1");
        let labels = t.labels(&marks);
        assert_eq!(
            labels[card(&t, "x1", Some("a3")).id].needs,
            vec!["\u{e9}9 is up"]
        );
        assert_eq!(
            labels[card(&t, "x2", Some("a3")).id].needs,
            vec!["<\u{e9}9> is up"]
        );
        assert_eq!(
            labels[card(&t, "y1", Some("a4")).id].needs,
            vec!["\u{e9}9 is up"]
        );

        let b = t.bundled();
        let labels = b.labels(&marks);
        let bundle = card(&b, "x1, x2", Some("a3"));
        assert_eq!(labels[bundle.id].behavior, "x1, x2");
        // x1's need and x2's are one need now: what either filled in.
        assert_eq!(labels[bundle.id].needs, vec!["<\u{e9}9> is up"]);
        // a4's promise is not in the bundle and filled nothing in.
        assert_eq!(
            labels[card(&b, "y1", Some("a4")).id].needs,
            vec!["\u{e9}9 is up"]
        );
        assert_eq!(
            labels[card(&b, "\u{e9}9 is up", Some("\u{e9}9")).id].behavior,
            "<\u{e9}9> is up"
        );
    }

    #[test]
    fn a_trail_read_back_is_labelled_plainly() {
        let tracker = crate::marks::tracker(crate::marks::IAM);
        let marks = Marks::new(&tracker);
        let t = trail(&tracker, "appname", "Read Secret Contents appsecret by appname");
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("spans"), "{}", json);
        let back: Trail = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cards, t.cards);
        assert!(back.labels(&marks).iter().all(|l| !l.behavior.contains('<')));
    }

    // --- Fold: Unneeded alternatives ---------------------------------------

    #[test]
    fn trail_with_folds_unneeded_alternatives() {
        let conditional = |name: &str, conditions: &[&str]| {
            Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
        };
        let mut tracker = Tracker::new();
        tracker.add_agent(
            Agent::build("x").with_wants(vec![Behavior::build("b1"), Behavior::build("b2")]),
        );
        tracker.add_agent(Agent::build("good").with_provides(vec![Behavior::build("b1")]));
        tracker.add_agent(Agent::build("alt").with_provides(vec![
            conditional("b1", &["c", "d"]),
            conditional("b2", &["c"]),
        ]));
        tracker.add_agent(Agent::build("cprov").with_provides(vec![Behavior::build("c")]));

        let full = trail_with(&tracker, "x", "b1", false);
        assert_eq!(full, trail(&tracker, "x", "b1"));
        assert_eq!(full.promise_count(), 4);
        assert_eq!(full.broken_count(), 2);

        let folded = trail_with(&tracker, "x", "b1", true);
        assert!(folded.satisfied);
        assert_eq!(folded.promise_count(), 1);
        assert_eq!(folded.broken_count(), 0);
        assert_eq!(folded.cards.len(), 2);
        card(&folded, "b1", Some("good"));

        // a want kept only through the alternative's other promise is whole
        let b2 = trail_with(&tracker, "x", "b2", true);
        assert_eq!(b2, trail(&tracker, "x", "b2"));
    }
}
