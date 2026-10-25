//! TraceView component - one want, read left to right.
//!
//! The resolution tree the Detailed view prints as nested text is laid out
//! here as columns: the want, who promises it, what that rests on, and so on
//! to the right. Each promise appears once; where the tree would repeat it,
//! the card that reached it again shows a pointer back instead.

use leptos::prelude::*;
use promise_tracker::trace::{trail, wants, CardKind, Trail, TrailCard, WantSummary};
use promise_tracker::Tracker;
use serde::Serialize;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;

use super::view_frame::ViewFrame;
use crate::utils::{escape_html, generated_at, html_document, slugify};

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

/// Everything the status strip says about a trail.
fn trail_summary(trail: &Trail) -> String {
    let promises = trail.cards.len().saturating_sub(1);
    let broken = trail.broken().len();
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

/// Other agents promising the same behavior as this card.
fn other_offers(trail: &Trail, card: &TrailCard) -> Vec<String> {
    trail
        .cards
        .iter()
        .filter(|c| c.kind == CardKind::Offer && c.behavior == card.behavior && c.id != card.id)
        .filter_map(|c| c.agent.clone())
        .collect()
}

/// The trail as static markup for the downloaded page: the same columns and
/// cards, without the hover and selection.
fn trail_html(trail: &Trail, summary: &str) -> String {
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
            out.push_str(&format!(
                "<div class=\"trail-card {}\"><div class=\"trail-card-behavior\">{}</div><div class=\"trail-card-agent\">{}</div>",
                card_status_class(card),
                escape_html(&card.behavior),
                escape_html(&card_agent_line(card))
            ));
            if !card.needs.is_empty() {
                out.push_str("<div class=\"trail-needs\">");
                for need in &card.needs {
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
                        escape_html(&need.behavior)
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
    #[prop(into)] show_self_promises: Signal<bool>,
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
        Some(if show_self_promises.get() {
            t
        } else {
            t.without_own_promises()
        })
    });

    // Card selection is by id, which changes with the trail, so it resets with it.
    let (selected_card, set_selected_card) = signal::<Option<usize>>(None);
    let (lit_card, set_lit_card) = signal::<Option<usize>>(None);
    Effect::new(move |_| {
        let _ = current_trail.get();
        set_selected_card.set(None);
        set_lit_card.set(None);
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
        let self_state = if show_self_promises.get() {
            "self-fulfilling promises shown"
        } else {
            "self-fulfilling promises folded"
        };
        format!("{} - generated {}", self_state, generated_at())
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
        let body = trail_html(&t, &summary.get_untracked());
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
                    </div>
                    {move || {
                        let groups = grouped_wants.get();
                        if groups.is_empty() {
                            return view! {
                                <div class="trail-muted" style="padding: 0.5rem;">
                                    "No wants in this simulation."
                                </div>
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
                                                let label = w.behavior.clone();
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
                        let columns = (0..width)
                            .map(|depth| {
                                let cards: Vec<TrailCard> =
                                    t.column(depth).into_iter().cloned().collect();
                                let trail_for_cards = t.clone();
                                view! {
                                    <div class="trail-column">
                                        <div class="trail-column-head">{column_heading(depth)}</div>
                                        {cards
                                            .into_iter()
                                            .map(|card| {
                                                let id = card.id;
                                                let status = card_status_class(&card);
                                                let agent_line = card_agent_line(&card);
                                                let behavior = card.behavior.clone();
                                                let needs = card.needs.clone();
                                                // On the chain in front, or faded behind it.
                                                let on_chain = move || {
                                                    chain.get().map(|c| c.contains(&id)).unwrap_or(false)
                                                };
                                                let off_chain = move || {
                                                    chain.get().map(|c| !c.contains(&id)).unwrap_or(false)
                                                };
                                                view! {
                                                    <button
                                                        type="button"
                                                        class=format!("trail-card {}", status)
                                                        data-card=id.to_string()
                                                        class:trail-card-selected=move || {
                                                            selected_card.get() == Some(id)
                                                        }
                                                        class:trail-card-lit=on_chain
                                                        class:trail-card-faded=off_chain
                                                        on:click=move |_| set_selected_card.set(Some(id))
                                                        on:mouseenter=move |_| set_lit_card.set(Some(id))
                                                        on:mouseleave=move |_| set_lit_card.set(None)
                                                    >
                                                        <span class="trail-card-behavior">{behavior}</span>
                                                        <span class="trail-card-agent">{agent_line}</span>
                                                        {if needs.is_empty() {
                                                            None
                                                        } else {
                                                            let chips = needs
                                                                .into_iter()
                                                                .map(|need| {
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
                                                                        need.behavior
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
                                                    </button>
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
                                    CardKind::Offer if card.satisfied => "Promise, held",
                                    CardKind::Offer => "Promise, not held",
                                };
                                let (agent_comment, behavior_comment) = tracker
                                    .get_untracked()
                                    .map(|tr| {
                                        let agent = card.agent.clone().unwrap_or_default();
                                        (
                                            tr.get_agent_comment(&agent),
                                            if card.kind == CardKind::Offer {
                                                tr.get_behavior_comment(&agent, &card.behavior)
                                            } else {
                                                None
                                            },
                                        )
                                    })
                                    .unwrap_or((None, None));
                                let conditions: Vec<(String, bool, bool)> = card
                                    .needs
                                    .iter()
                                    .map(|n| (n.behavior.clone(), t.cards[n.card].satisfied, n.own))
                                    .collect();
                                let dependents: Vec<String> = needed_by(&t, id)
                                    .into_iter()
                                    .map(|c| match &c.agent {
                                        Some(a) if c.kind == CardKind::Offer => {
                                            format!("{} by {}", c.behavior, a)
                                        }
                                        Some(a) => format!("{} wanted by {}", c.behavior, a),
                                        None => c.behavior.clone(),
                                    })
                                    .collect();
                                let others = other_offers(&t, &card);
                                let agent_line = card_agent_line(&card);
                                view! {
                                    <div class="trail-inspector">
                                        <div class="trail-inspector-kind">{kind_label}</div>
                                        <div class="trail-inspector-title">{card.behavior.clone()}</div>
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

/// Helper function to schedule a callback using requestAnimationFrame
fn request_animation_frame<F>(f: F) -> Result<i32, JsValue>
where
    F: FnOnce() + 'static,
{
    let window = web_sys::window().expect("no global window exists");
    let closure = Closure::once_into_js(f);
    window.request_animation_frame(closure.as_ref().unchecked_ref())
}
