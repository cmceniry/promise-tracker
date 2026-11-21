//! TraceView component - one want, read left to right.
//!
//! The resolution tree the Detailed view prints as nested text is laid out
//! here as columns: the want, who promises it, what that rests on, and so on
//! to the right. Each promise appears once; where the tree would repeat it,
//! the card that reached it again shows a pointer back instead.

use leptos::prelude::*;
use promise_tracker::marks::Marks;
use promise_tracker::trace::{trail, wants, CardKind, CardLabels, Trail, TrailCard, WantSummary};
use promise_tracker::Tracker;
use serde::Serialize;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;

use super::display_options::ViewOptions;
use super::view_frame::ViewFrame;
use crate::utils::{escape_html, generated_at, html_document, search_key, slugify};

// JS interop for the connector overlay in index.html
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = draw_trail_links)]
    fn js_draw_trail_links(container_id: &str, links: JsValue, chain: JsValue);

    #[wasm_bindgen(js_name = destroy_trail_links)]
    fn js_destroy_trail_links(container_id: &str);
}

/// One arrow between two cards, for the overlay.
#[derive(Serialize)]
struct Link {
    from: usize,
    to: usize,
    /// The card pointed at is not held.
    broken: bool,
    /// Points back at a card that was already on the trail.
    reference: bool,
}

fn links_of(trail: &Trail) -> Vec<Link> {
    trail
        .cards
        .iter()
        .flat_map(|card| {
            card.needs.iter().map(move |need| Link {
                from: card.id,
                to: need.card,
                broken: !trail.cards[need.card].satisfied,
                reference: need.reference,
            })
        })
        .collect()
}

/// Everything upstream and downstream of one card: the whole chain through
/// it, read left to right.
fn chain_through(trail: &Trail, id: usize) -> HashSet<usize> {
    let mut chain: HashSet<usize> = HashSet::new();
    chain.insert(id);
    // Downstream: what it rests on.
    let mut queue = vec![id];
    while let Some(c) = queue.pop() {
        for need in &trail.cards[c].needs {
            if chain.insert(need.card) {
                queue.push(need.card);
            }
        }
    }
    // Upstream: what rests on it.
    let mut queue = vec![id];
    while let Some(c) = queue.pop() {
        for parent in needed_by(trail, c) {
            if chain.insert(parent.id) {
                queue.push(parent.id);
            }
        }
    }
    chain
}

/// Heading over a column, by depth.
fn column_heading(depth: usize) -> &'static str {
    match depth {
        0 => "Want",
        1 => "Promised by",
        _ => "Which needs",
    }
}

/// The CSS class that colors a card.
fn card_status_class(card: &TrailCard) -> &'static str {
    match card.kind {
        CardKind::Want => "trail-card-want",
        CardKind::Missing => "trail-card-missing",
        CardKind::Offer if card.satisfied => "trail-card-held",
        CardKind::Offer => "trail-card-broken",
    }
}

/// The second line of a card: who is behind it.
fn card_agent_line(card: &TrailCard) -> String {
    match (card.kind, card.agent.as_deref()) {
        (CardKind::Want, Some(agent)) => format!("wanted by {}", agent),
        (CardKind::Missing, _) => "nobody promises it".to_string(),
        (_, Some(agent)) => format!("by {}", agent),
        (_, None) => String::new(),
    }
}

/// The behavior line of a card: a bundle counts its promises. `behavior` is
/// what the card says its behavior is — its name, or its marked form.
fn card_behavior_line(card: &TrailCard, behavior: &str) -> String {
    if card.promises.is_empty() {
        behavior.to_string()
    } else {
        format!("{} \u{00d7}{}", behavior, card.count())
    }
}

/// What a card says, falling back to its plain names if the labels were
/// made for some other trail.
fn labels_for(labels: &[CardLabels], card: &TrailCard) -> CardLabels {
    labels
        .get(card.id)
        .filter(|l| l.needs.len() == card.needs.len() && l.promises.len() == card.promises.len())
        .cloned()
        .unwrap_or_else(|| CardLabels {
            behavior: card.behavior.clone(),
            promises: card.promises.iter().map(|p| p.behavior.clone()).collect(),
            needs: card.needs.iter().map(|n| n.behavior.clone()).collect(),
        })
}

/// Everything the status strip says about a trail.
fn trail_summary(trail: &Trail) -> String {
    let promises = trail.promise_count();
    let broken = trail.broken_count();
    if trail.cards.len() <= 1 {
        return "Nothing to rest on.".to_string();
    }
    if trail.satisfied {
        format!(
            "Held. {} promise{} on this trail, {}.",
            promises,
            if promises == 1 { "" } else { "s" },
            if broken == 0 {
                "all kept".to_string()
            } else {
                format!("{} not kept on a side branch", broken)
            }
        )
    } else {
        format!(
            "Not held. {} promise{} on this trail, {} not kept.",
            promises,
            if promises == 1 { "" } else { "s" },
            broken
        )
    }
}

/// The cards whose needs point at this one.
fn needed_by(trail: &Trail, id: usize) -> Vec<&TrailCard> {
    trail
        .cards
        .iter()
        .filter(|c| c.needs.iter().any(|n| n.card == id))
        .collect()
}

/// Other agents promising a behavior this card promises.
fn other_offers(trail: &Trail, card: &TrailCard) -> Vec<String> {
    let mine = card.behaviors();
    let mut others: Vec<String> = trail
        .cards
        .iter()
        .filter(|c| c.kind == CardKind::Offer && c.id != card.id && c.agent != card.agent)
        .filter(|c| c.behaviors().iter().any(|b| mine.contains(b)))
        .filter_map(|c| c.agent.clone())
        .collect();
    others.sort();
    others.dedup();
    others
}

/// The trail as static markup for the downloaded page: the same columns and
/// cards, without the hover and selection.
fn trail_html(trail: &Trail, labels: &[CardLabels], summary: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<div class=\"trail-status {}\">{}</div>\n<div class=\"trail-columns\">\n",
        if trail.satisfied {
            "trail-status-held"
        } else {
            "trail-status-broken"
        },
        escape_html(summary)
    ));
    for depth in 0..trail.width() {
        out.push_str(&format!(
            "<div class=\"trail-column\"><div class=\"trail-column-head\">{}</div>\n",
            column_heading(depth)
        ));
        for card in trail.column(depth) {
            let said = labels_for(labels, card);
            out.push_str(&format!(
                "<div class=\"trail-card {}\"><div class=\"trail-card-behavior\">{}</div><div class=\"trail-card-agent\">{}</div>",
                card_status_class(card),
                escape_html(&card_behavior_line(card, &said.behavior)),
                escape_html(&card_agent_line(card))
            ));
            if !card.promises.is_empty() {
                out.push_str("<ul class=\"trail-card-promises\">");
                for (p, label) in card.promises.iter().zip(&said.promises) {
                    out.push_str(&format!(
                        "<li{}>{}{}</li>",
                        if p.satisfied {
                            ""
                        } else {
                            " class=\"trail-text-broken\""
                        },
                        escape_html(label),
                        if p.satisfied { "" } else { " \u{2717}" }
                    ));
                }
                out.push_str("</ul>");
            }
            if !card.needs.is_empty() {
                out.push_str("<div class=\"trail-needs\">");
                for (need, label) in card.needs.iter().zip(&said.needs) {
                    let mut classes = vec!["trail-need"];
                    if need.reference {
                        classes.push("trail-need-ref");
                    }
                    if need.own {
                        classes.push("trail-need-own");
                    }
                    if !trail.cards[need.card].satisfied {
                        classes.push("trail-need-broken");
                    }
                    out.push_str(&format!(
                        "<span class=\"{}\">{}{}</span>",
                        classes.join(" "),
                        if need.reference { "\u{21a9} " } else { "" },
                        escape_html(label)
                    ));
                }
                out.push_str("</div>");
            }
            out.push_str("</div>\n");
        }
        out.push_str("</div>\n");
    }
    out.push_str("</div>\n");
    out
}

/// TraceView component - the want list, the trail, and an inspector for the
/// selected card.
#[component]
pub fn TraceView(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    /// The agent whose want is traced; "---" for none
    selected_component: ReadSignal<String>,
    /// The want traced; "---" for none
    selected_behavior: ReadSignal<String>,
    /// Picks a want from the list
    on_select: Callback<(String, String)>,
    /// The display options shared by every view
    #[prop(into)]
    options: Signal<ViewOptions>,
    /// Which simulation this panel belongs to, for the export's header
    sim_label: String,
) -> impl IntoView {
    let want_list = Memo::new(move |_| tracker.get().map(|t| wants(&t)).unwrap_or_default());

    let selection = Memo::new(move |_| {
        let agent = selected_component.get();
        let behavior = selected_behavior.get();
        if agent == "---" || behavior == "---" {
            None
        } else {
            Some((agent, behavior))
        }
    });

    // The walk does not depend on the display options, so it is memoized
    // apart from the fold below.
    let full_trail = Memo::new(move |_| {
        let (agent, behavior) = selection.get()?;
        let t = tracker.get()?;
        if !t.has_agent(agent.clone()) || !t.get_agent_wants(agent.clone()).contains(&behavior) {
            return None;
        }
        Some(trail(&t, &agent, &behavior))
    });

    let current_trail = Memo::new(move |_| {
        let t = full_trail.get()?;
        let o = options.get();
        let t = if o.fold_self {
            t.without_own_promises()
        } else {
            t
        };
        Some(if o.bundle { t.bundled() } else { t })
    });

    // Where substituted values sit, read only while marking is on.
    let marks = Memo::new(move |_| {
        options
            .get()
            .mark_bound
            .then(|| tracker.get().map(|t| Marks::new(&t)))
            .flatten()
    });
    // What each card says, by card id: plain names, or marked ones. The
    // cards keep plain names either way, for selection and lookups.
    let card_labels = Memo::new(move |_| {
        current_trail
            .get()
            .map(|t| t.labels(&marks.get().unwrap_or_default()))
            .unwrap_or_default()
    });

    // Card selection is by id, which changes with the trail. When the trail
    // changes (a followed contract moved, an option flipped) the chosen card
    // is found again by what it stands for; it is let go only if it is gone.
    let (selected_card, set_selected_card) = signal::<Option<usize>>(None);
    let (lit_card, set_lit_card) = signal::<Option<usize>>(None);
    // Choosing the chosen card again lets it go, which also closes a bundle.
    let toggle = move |id: usize| {
        set_selected_card.update(|s| *s = if *s == Some(id) { None } else { Some(id) })
    };
    Effect::new(move |previous: Option<Option<Trail>>| {
        let next = current_trail.get();
        let carried = previous.flatten().and_then(|old| {
            let next = next.as_ref()?;
            // Another want is another trail: start it fresh.
            if old.agent != next.agent || old.behavior != next.behavior {
                return None;
            }
            let chosen = old.cards.get(selected_card.get_untracked()?)?;
            next.cards
                .iter()
                .find(|c| {
                    c.depth == chosen.depth
                        && c.kind == chosen.kind
                        && c.behavior == chosen.behavior
                        && c.agent == chosen.agent
                })
                .map(|c| c.id)
        });
        set_selected_card.set(carried);
        set_lit_card.set(None);
        next
    });

    // The chain in front: through the hovered card, else the selected one.
    let chain = Memo::new(move |_| {
        let t = current_trail.get()?;
        let focus = lit_card.get().or(selected_card.get())?;
        t.cards.get(focus)?;
        Some(chain_through(&t, focus))
    });

    // The connector overlay is drawn from the DOM, so it runs after the
    // cards are in place and again whenever the chain changes.
    let columns_id = format!("trail-columns-{}", slugify(&sim_label));
    let columns_id_for_view = columns_id.clone();
    let columns_id_for_cleanup = columns_id.clone();
    Effect::new(move |_| {
        let Some(t) = current_trail.get() else {
            js_destroy_trail_links(&columns_id);
            return;
        };
        let links = serde_wasm_bindgen::to_value(&links_of(&t)).unwrap_or(JsValue::NULL);
        let chain_ids: Option<Vec<usize>> = chain.get().map(|c| c.into_iter().collect());
        let chain_value = serde_wasm_bindgen::to_value(&chain_ids).unwrap_or(JsValue::NULL);
        let id = columns_id.clone();
        let _ = request_animation_frame(move || {
            let _ = request_animation_frame(move || js_draw_trail_links(&id, links, chain_value));
        });
    });
    on_cleanup(move || js_destroy_trail_links(&columns_id_for_cleanup));

    let summary = Memo::new(move |_| {
        current_trail
            .get()
            .map(|t| trail_summary(&t))
            .unwrap_or_default()
    });

    let sim_for_title = sim_label.clone();
    let sim_for_file = sim_label.clone();
    let frame_title = Signal::derive(move || match selection.get() {
        Some((agent, behavior)) => {
            format!(
                "Trace - Simulation {} - {} wants {}",
                sim_for_title, agent, behavior
            )
        }
        None => format!("Trace - Simulation {}", sim_for_title),
    });
    let frame_subtitle = Signal::derive(move || {
        format!(
            "{} - generated {}",
            options.get().describe(),
            generated_at()
        )
    });
    let frame_filename = Signal::derive(move || match selection.get() {
        Some((agent, behavior)) => format!(
            "promise-tracker-trace-{}-{}-{}",
            slugify(&sim_for_file),
            slugify(&agent),
            slugify(&behavior)
        ),
        None => format!("promise-tracker-trace-{}", slugify(&sim_for_file)),
    });

    let export = Callback::new(move |_: ()| {
        let t = current_trail.get_untracked()?;
        let body = trail_html(&t, &card_labels.get_untracked(), &summary.get_untracked());
        Some(html_document(
            &frame_title.get_untracked(),
            &frame_subtitle.get_untracked(),
            "",
            &body,
        ))
    });

    // The want list, grouped by agent in the order the tracker gives them.
    let grouped_wants = Memo::new(move |_| {
        let mut groups: Vec<(String, Vec<WantSummary>)> = vec![];
        for want in want_list.get() {
            match groups.last_mut() {
                Some((agent, items)) if *agent == want.agent => items.push(want),
                _ => groups.push((want.agent.clone(), vec![want])),
            }
        }
        groups
    });

    // Search text for the want list; it narrows the list, not the selection.
    let want_query = RwSignal::new(String::new());
    let shown_wants = Memo::new(move |_| {
        let query = want_query.get();
        grouped_wants.with(|groups| filter_wants(groups, &query))
    });

    view! {
        <ViewFrame
            title=frame_title
            subtitle=frame_subtitle
            filename=frame_filename
            export=export
        >
            <div class="trail-layout">
                <div class="trail-wants">
                    <div class="trail-wants-head">
                        "Wants"
                        <span class="trail-muted">
                            {move || {
                                let all = want_list.get();
                                let broken = all.iter().filter(|w| !w.satisfied).count();
                                if all.is_empty() {
                                    String::new()
                                } else if broken == 0 {
                                    format!("{}, all held", all.len())
                                } else {
                                    format!("{}, {} not held", all.len(), broken)
                                }
                            }}
                        </span>
                        <input
                            type="search"
                            class="form-control form-control-sm"
                            placeholder="Find a want or agent"
                            aria-label="Find a want or agent"
                            prop:value=move || want_query.get()
                            on:input=move |ev| want_query.set(event_target_value(&ev))
                        />
                    </div>
                    {move || {
                        let groups = shown_wants.get();
                        if groups.is_empty() {
                            let text = if grouped_wants.with(|g| g.is_empty()) {
                                "No wants in this simulation."
                            } else {
                                "No match."
                            };
                            return view! {
                                <div class="trail-muted" style="padding: 0.5rem;">{text}</div>
                            }
                                .into_any();
                        }
                        groups
                            .into_iter()
                            .map(|(agent, items)| {
                                let agent_label = agent.clone();
                                let broken = items.iter().filter(|w| !w.satisfied).count();
                                view! {
                                    <div class="trail-wants-group">
                                        <div class="trail-wants-agent">
                                            <span>{agent_label}</span>
                                            <span class="trail-muted">
                                                {if broken == 0 {
                                                    format!("{}", items.len())
                                                } else {
                                                    format!("{} \u{00b7} {} not held", items.len(), broken)
                                                }}
                                            </span>
                                        </div>
                                        {items
                                            .into_iter()
                                            .map(|w| {
                                                let agent_for_click = w.agent.clone();
                                                let behavior_for_click = w.behavior.clone();
                                                let agent_for_active = w.agent.clone();
                                                let behavior_for_active = w.behavior.clone();
                                                // The want as its agent declares it,
                                                // marked when asked for.
                                                let label = marks
                                                    .with(|m| {
                                                        m.as_ref().map(|m| m.need(&w.agent, &w.behavior))
                                                    })
                                                    .unwrap_or_else(|| w.behavior.clone());
                                                let held = w.satisfied;
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="trail-want"
                                                        class:trail-want-active=move || {
                                                            selection.get()
                                                                == Some((
                                                                    agent_for_active.clone(),
                                                                    behavior_for_active.clone(),
                                                                ))
                                                        }
                                                        class:trail-want-broken=!held
                                                        on:click=move |_| {
                                                            on_select
                                                                .run((
                                                                    agent_for_click.clone(),
                                                                    behavior_for_click.clone(),
                                                                ))
                                                        }
                                                    >
                                                        <span
                                                            class="trail-dot"
                                                            class:trail-dot-broken=!held
                                                        ></span>
                                                        <span class="trail-want-label">{label}</span>
                                                    </button>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()
                            .into_any()
                    }}
                </div>

                <div class="trail-main">
                    {move || {
                        let Some(t) = current_trail.get() else {
                            return view! {
                                <div class="trail-empty">
                                    {if selection.get().is_some() {
                                        "That want is not in this simulation."
                                    } else {
                                        "Pick a want on the left to see what it rests on."
                                    }}
                                </div>
                            }
                                .into_any();
                        };
                        let status_class = if t.satisfied {
                            "trail-status trail-status-held"
                        } else {
                            "trail-status trail-status-broken"
                        };
                        let width = t.width();
                        let labels = card_labels.get();
                        let labels_for_inspector = labels.clone();
                        let columns = (0..width)
                            .map(|depth| {
                                let cards: Vec<TrailCard> =
                                    t.column(depth).into_iter().cloned().collect();
                                let trail_for_cards = t.clone();
                                let labels = labels.clone();
                                view! {
                                    <div class="trail-column">
                                        <div class="trail-column-head">{column_heading(depth)}</div>
                                        {cards
                                            .into_iter()
                                            .map(|card| {
                                                let id = card.id;
                                                let status = card_status_class(&card);
                                                let agent_line = card_agent_line(&card);
                                                let said = labels_for(&labels, &card);
                                                let behavior = card_behavior_line(&card, &said.behavior);
                                                let promises: Vec<(String, bool)> = said
                                                    .promises
                                                    .iter()
                                                    .cloned()
                                                    .zip(card.promises.iter().map(|p| p.satisfied))
                                                    .collect();
                                                let expandable = !promises.is_empty();
                                                let needs: Vec<_> = card
                                                    .needs
                                                    .iter()
                                                    .cloned()
                                                    .zip(said.needs.iter().cloned())
                                                    .collect();
                                                // On the chain in front, or faded behind it.
                                                let on_chain = move || {
                                                    chain.get().map(|c| c.contains(&id)).unwrap_or(false)
                                                };
                                                let off_chain = move || {
                                                    chain.get().map(|c| !c.contains(&id)).unwrap_or(false)
                                                };
                                                view! {
                                                    // A div acting as a button, not a <button>: a
                                                    // bundle's promise list goes inside it.
                                                    <div
                                                        role="button"
                                                        tabindex="0"
                                                        class=format!("trail-card {}", status)
                                                        data-card=id.to_string()
                                                        class:trail-card-selected=move || {
                                                            selected_card.get() == Some(id)
                                                        }
                                                        aria-expanded=move || {
                                                            expandable
                                                                .then(|| (selected_card.get() == Some(id)).to_string())
                                                        }
                                                        class:trail-card-lit=on_chain
                                                        class:trail-card-faded=off_chain
                                                        on:click=move |_| toggle(id)
                                                        on:keydown=move |ev| {
                                                            if ev.key() == "Enter" || ev.key() == " " {
                                                                ev.prevent_default();
                                                                toggle(id);
                                                            }
                                                        }
                                                        on:mouseenter=move |_| set_lit_card.set(Some(id))
                                                        on:mouseleave=move |_| set_lit_card.set(None)
                                                    >
                                                        <span class="trail-card-behavior">{behavior}</span>
                                                        <span class="trail-card-agent">{agent_line}</span>
                                                        // A bundle opens to its promises when chosen.
                                                        {(!promises.is_empty()).then(|| {
                                                            view! {
                                                                <Show when=move || selected_card.get() == Some(id)>
                                                                    <ul class="trail-card-promises">
                                                                        {promises
                                                                            .iter()
                                                                            .map(|(label, satisfied)| {
                                                                                let broken = !satisfied;
                                                                                view! {
                                                                                    <li class:trail-text-broken=broken>
                                                                                        {label.clone()}
                                                                                        {if broken { " \u{2717}" } else { "" }}
                                                                                    </li>
                                                                                }
                                                                            })
                                                                            .collect::<Vec<_>>()}
                                                                    </ul>
                                                                </Show>
                                                            }
                                                        })}
                                                        {if needs.is_empty() {
                                                            None
                                                        } else {
                                                            let chips = needs
                                                                .into_iter()
                                                                .map(|(need, label)| {
                                                                    let target = need.card;
                                                                    let broken = !trail_for_cards.cards[target].satisfied;
                                                                    let title = if need.reference {
                                                                        "Already on the trail; click to jump to it"
                                                                    } else if need.own {
                                                                        "Met by this agent itself"
                                                                    } else {
                                                                        "Click to select"
                                                                    };
                                                                    let text = format!(
                                                                        "{}{}",
                                                                        if need.reference { "\u{21a9} " } else { "" },
                                                                        label
                                                                    );
                                                                    view! {
                                                                        <span
                                                                            class="trail-need"
                                                                            class:trail-need-ref=need.reference
                                                                            class:trail-need-own=need.own
                                                                            class:trail-need-broken=broken
                                                                            title=title
                                                                            on:click=move |ev| {
                                                                                ev.stop_propagation();
                                                                                set_selected_card.set(Some(target));
                                                                            }
                                                                        >
                                                                            {text}
                                                                        </span>
                                                                    }
                                                                })
                                                                .collect::<Vec<_>>();
                                                            Some(view! { <span class="trail-needs">{chips}</span> })
                                                        }}
                                                    </div>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>();

                        let inspector = {
                            let t = t.clone();
                            move || {
                                let Some(id) = selected_card.get() else {
                                    return view! {
                                        <div class="trail-inspector trail-muted">
                                            "Click a card for its comment, its conditions and who else rests on it."
                                        </div>
                                    }
                                        .into_any();
                                };
                                let Some(card) = t.cards.get(id).cloned() else {
                                    return view! { <div class="trail-inspector"></div> }.into_any();
                                };
                                let kind_label = match card.kind {
                                    CardKind::Want => "Want",
                                    CardKind::Missing => "Missing",
                                    CardKind::Offer if !card.promises.is_empty() && card.satisfied => {
                                        "Promises, all held"
                                    }
                                    CardKind::Offer if !card.promises.is_empty() => "Promises, not all held",
                                    CardKind::Offer if card.satisfied => "Promise, held",
                                    CardKind::Offer => "Promise, not held",
                                };
                                let (agent_comment, behavior_comment) = tracker
                                    .get_untracked()
                                    .map(|tr| {
                                        let agent = card.agent.clone().unwrap_or_default();
                                        (
                                            tr.get_agent_comment(&agent),
                                            if card.kind == CardKind::Offer && card.promises.is_empty() {
                                                tr.get_behavior_comment(&agent, &card.behavior)
                                            } else {
                                                None
                                            },
                                        )
                                    })
                                    .unwrap_or((None, None));
                                let said = labels_for(&labels_for_inspector, &card);
                                let conditions: Vec<(String, bool, bool)> = card
                                    .needs
                                    .iter()
                                    .zip(&said.needs)
                                    .map(|(n, label)| (label.clone(), t.cards[n.card].satisfied, n.own))
                                    .collect();
                                let dependents: Vec<String> = needed_by(&t, id)
                                    .into_iter()
                                    .map(|c| {
                                        let behavior = labels_for(&labels_for_inspector, c).behavior;
                                        match &c.agent {
                                            Some(a) if c.kind == CardKind::Offer => {
                                                format!("{} by {}", behavior, a)
                                            }
                                            Some(a) => format!("{} wanted by {}", behavior, a),
                                            None => behavior,
                                        }
                                    })
                                    .collect();
                                let others = other_offers(&t, &card);
                                let agent_line = card_agent_line(&card);
                                view! {
                                    <div class="trail-inspector">
                                        <div class="trail-inspector-kind">{kind_label}</div>
                                        <div class="trail-inspector-title">{card_behavior_line(&card, &said.behavior)}</div>
                                        <div class="trail-muted">{agent_line}</div>
                                        {behavior_comment.map(|c| view! { <p class="trail-inspector-comment">{c}</p> })}
                                        {agent_comment.map(|c| view! {
                                            <p class="trail-inspector-comment trail-muted">
                                                <b>"Agent: "</b>{c}
                                            </p>
                                        })}
                                        <div class="trail-inspector-grid">
                                            <div>
                                                <div class="trail-inspector-label">"Rests on"</div>
                                                {if conditions.is_empty() {
                                                    view! { <div class="trail-muted">"nothing: a leaf"</div> }.into_any()
                                                } else {
                                                    view! {
                                                        <ul class="trail-inspector-list">
                                                            {conditions
                                                                .into_iter()
                                                                .map(|(b, held, own)| {
                                                                    view! {
                                                                        <li class:trail-text-broken=!held>
                                                                            {b}
                                                                            {if own { " (its own)" } else { "" }}
                                                                            {if held { "" } else { " \u{2717}" }}
                                                                        </li>
                                                                    }
                                                                })
                                                                .collect::<Vec<_>>()}
                                                        </ul>
                                                    }
                                                        .into_any()
                                                }}
                                            </div>
                                            <div>
                                                <div class="trail-inspector-label">"Needed by"</div>
                                                {if dependents.is_empty() {
                                                    view! { <div class="trail-muted">"nothing on this trail"</div> }.into_any()
                                                } else {
                                                    view! {
                                                        <ul class="trail-inspector-list">
                                                            {dependents.into_iter().map(|d| view! { <li>{d}</li> }).collect::<Vec<_>>()}
                                                        </ul>
                                                    }
                                                        .into_any()
                                                }}
                                            </div>
                                            <div>
                                                <div class="trail-inspector-label">"Other offers"</div>
                                                {if others.is_empty() {
                                                    view! { <div class="trail-muted">"none on this trail"</div> }.into_any()
                                                } else {
                                                    view! {
                                                        <ul class="trail-inspector-list">
                                                            {others.into_iter().map(|o| view! { <li>{o}</li> }).collect::<Vec<_>>()}
                                                        </ul>
                                                    }
                                                        .into_any()
                                                }}
                                            </div>
                                        </div>
                                    </div>
                                }
                                    .into_any()
                            }
                        };

                        view! {
                            <div class=status_class>{summary.get()}</div>
                            <div class="trail-columns" id=columns_id_for_view.clone()>{columns}</div>
                            {inspector}
                        }
                            .into_any()
                    }}
                </div>
            </div>
        </ViewFrame>
    }
}

/// Narrow the grouped want list to a search: an agent whose name matches keeps
/// all its wants, any other agent keeps the wants whose behavior matches.
/// Compared by `search_key`: case-insensitive and blind to `{}<>`; a query
/// that is blank once normalised keeps everything.
fn filter_wants(
    groups: &[(String, Vec<WantSummary>)],
    query: &str,
) -> Vec<(String, Vec<WantSummary>)> {
    let q = search_key(query);
    if q.is_empty() {
        return groups.to_vec();
    }
    groups
        .iter()
        .filter_map(|(agent, items)| {
            if search_key(agent).contains(&q) {
                return Some((agent.clone(), items.clone()));
            }
            let kept: Vec<WantSummary> = items
                .iter()
                .filter(|w| search_key(&w.behavior).contains(&q))
                .cloned()
                .collect();
            (!kept.is_empty()).then(|| (agent.clone(), kept))
        })
        .collect()
}

/// Helper function to schedule a callback using requestAnimationFrame
fn request_animation_frame<F>(f: F) -> Result<i32, JsValue>
where
    F: FnOnce() + 'static,
{
    let window = web_sys::window().expect("no global window exists");
    let closure = Closure::once_into_js(f);
    window.request_animation_frame(closure.as_ref().unchecked_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want(agent: &str, behavior: &str) -> WantSummary {
        WantSummary {
            agent: agent.to_string(),
            behavior: behavior.to_string(),
            satisfied: true,
        }
    }

    fn names(groups: &[(String, Vec<WantSummary>)]) -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .map(|(a, ws)| (a.clone(), ws.iter().map(|w| w.behavior.clone()).collect()))
            .collect()
    }

    #[test]
    fn filter_wants_matches_agents_and_behaviors() {
        let groups = vec![
            ("web".to_string(), vec![want("web", "db-conn"), want("web", "cache")]),
            ("Database".to_string(), vec![want("Database", "disk")]),
            ("queue".to_string(), vec![want("queue", "disk"), want("queue", "net")]),
        ];
        // Blank keeps everything
        assert_eq!(filter_wants(&groups, "  ").len(), 3);
        // A behavior match keeps only that want; an agent match keeps the agent whole
        assert_eq!(
            names(&filter_wants(&groups, "DB")),
            vec![("web".to_string(), vec!["db-conn".to_string()])]
        );
        assert_eq!(
            names(&filter_wants(&groups, "data")),
            vec![("Database".to_string(), vec!["disk".to_string()])]
        );
        assert_eq!(
            names(&filter_wants(&groups, "disk")),
            vec![
                ("Database".to_string(), vec!["disk".to_string()]),
                ("queue".to_string(), vec!["disk".to_string()]),
            ]
        );
        assert!(filter_wants(&groups, "nope").is_empty());
    }

    #[test]
    fn filter_wants_ignores_braces_and_angle_brackets() {
        let groups = vec![
            (
                "<A1>".to_string(),
                vec![want("<A1>", "<A1> reads R1"), want("<A1>", "cache")],
            ),
            ("viewer".to_string(), vec![want("viewer", "{{?doc}} shows usage")]),
            ("plain".to_string(), vec![want("plain", "B2 reads R1")]),
        ];
        let behaviors = |q: &str| -> Vec<String> {
            filter_wants(&groups, q)
                .into_iter()
                .flat_map(|(_, ws)| ws.into_iter().map(|w| w.behavior))
                .collect()
        };
        // Marks in the text, none in the query
        assert_eq!(behaviors("a1 reads"), vec!["<A1> reads R1"]);
        assert_eq!(behaviors("?doc shows"), vec!["{{?doc}} shows usage"]);
        // Marks in the query, with or without them in the text
        assert_eq!(behaviors("{{?doc}}"), vec!["{{?doc}} shows usage"]);
        assert_eq!(behaviors("<B2> reads"), vec!["B2 reads R1"]);
        // An agent name matches the same way and keeps the agent whole
        assert_eq!(behaviors("{a1}"), vec!["<A1> reads R1", "cache"]);
        // Nothing left once stripped: keeps everything, like a blank query
        assert_eq!(filter_wants(&groups, " <>{} ").len(), 3);
    }
}
