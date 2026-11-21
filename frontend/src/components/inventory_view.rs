//! InventoryView - the contract's inventory for one simulation: what an
//! agent provides, who provides and uses a behavior, which wants nobody
//! keeps, and which promises nobody uses.
//!
//! The lists come from `promise_tracker::query`, the same queries behind the
//! CLI's `provides`, `providers`, `check-unsatisfied` and `unused`. The
//! inner tab and the selected agent and behavior are shared by every
//! simulation panel ([`InventoryState`]), so each panel shows the same
//! subject.

use leptos::ev;
use leptos::prelude::*;
use promise_tracker::query::{
    self, AgentProvides, BehaviorUse, CauseKind, ConditionStatus, ConsumerKind, QueryOptions,
    UnsatisfiedRow, UnusedBehavior,
};
use promise_tracker::marks::Marks;
use promise_tracker::Tracker;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen::JsCast;

use super::display_options::ViewOptions;
use crate::utils::search_key;

/// The four inner tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InventoryTab {
    #[default]
    Agents,
    Behaviors,
    Unsatisfied,
    Unused,
}

/// Where the agent selection goes when the SuperAgents fold flips, as
/// (selection, remembered member and the collective it was folded into).
///
/// Folding moves a member to the first collective that folds it and
/// remembers the member; unfolding returns to the remembered member when the
/// selection is still that collective. Any other selection stays put and the
/// memory is dropped.
fn follow_fold(
    selected: Option<String>,
    memory: Option<(String, String)>,
    folded: bool,
    folding: impl Fn(&str) -> Vec<String>,
) -> (Option<String>, Option<(String, String)>) {
    let Some(current) = selected else {
        return (None, None);
    };
    if folded {
        match folding(&current).into_iter().next() {
            Some(collective) => (Some(collective.clone()), Some((current, collective))),
            None => (Some(current), None),
        }
    } else {
        match memory {
            Some((member, collective)) if collective == current => (Some(member), None),
            _ => (Some(current), None),
        }
    }
}

/// The Inventory's selection, shared by every simulation panel. Lives in
/// ContractGrapher so it survives tab switches and panel rebuilds.
#[derive(Clone, Copy)]
pub struct InventoryState {
    pub tab: RwSignal<InventoryTab>,
    pub agent: RwSignal<Option<String>>,
    pub behavior: RwSignal<Option<String>>,
    /// The member the selection was folded away from, with the collective it
    /// went to; cleared by any selection the reader makes.
    unfolded_from: RwSignal<Option<(String, String)>>,
    /// The open peek popover, by the id of the name it hangs from. One at a
    /// time across all panels.
    peek: RwSignal<Option<usize>>,
}

impl InventoryState {
    pub fn new() -> Self {
        InventoryState {
            tab: RwSignal::new(InventoryTab::default()),
            agent: RwSignal::new(None),
            behavior: RwSignal::new(None),
            unfolded_from: RwSignal::new(None),
            peek: RwSignal::new(None),
        }
    }

    pub fn select_agent(&self, name: String) {
        self.unfolded_from.set(None);
        self.agent.set(Some(name));
    }

    pub fn select_behavior(&self, name: String) {
        self.behavior.set(Some(name));
    }

    /// Navigate: the Agents tab with `name` selected.
    pub fn open_agent(&self, name: String) {
        self.peek.set(None);
        self.select_agent(name);
        self.tab.set(InventoryTab::Agents);
    }

    /// Navigate: the Behaviors tab with `name` selected.
    pub fn open_behavior(&self, name: String) {
        self.peek.set(None);
        self.select_behavior(name);
        self.tab.set(InventoryTab::Behaviors);
    }

    /// Carry the agent selection across a flip of the SuperAgents fold.
    /// `tracker` may be built either way: folding_collectives does not
    /// depend on it.
    pub fn follow_fold(&self, tracker: &Tracker, folded: bool) {
        let selected = self.agent.get_untracked();
        let (next, memory) = follow_fold(
            selected.clone(),
            self.unfolded_from.get_untracked(),
            folded,
            |member| tracker.folding_collectives(member),
        );
        if next != selected {
            self.agent.set(next);
        }
        self.unfolded_from.set(memory);
    }
}

impl Default for InventoryState {
    fn default() -> Self {
        Self::new()
    }
}

/// What the Agents tab shows for the selected agent in one simulation.
#[derive(Debug, Clone, PartialEq)]
enum AgentView {
    Provides(AgentProvides),
    /// A SuperAgent that does not stand as an agent here (SuperAgents
    /// unfolded): its members.
    Members(String, Vec<String>),
    Absent(String),
}

/// What the Behaviors tab shows for the selected behavior in one simulation.
#[derive(Debug, Clone, PartialEq)]
enum BehaviorView {
    Use(BehaviorUse),
    Absent(String),
}

/// What a name in a list stands for.
#[derive(Debug, Clone, PartialEq)]
enum Subject {
    Agent(String),
    Behavior(String),
}

/// One panel's view of its simulation, for the names and rows inside it.
#[derive(Clone, Copy)]
struct Panel {
    state: InventoryState,
    tracker: Signal<Option<Tracker>>,
    qopts: Memo<QueryOptions>,
    /// Where substituted values sit in every behavior name, while "Mark:
    /// Bound values" is on; None while it is off. Only ever read to choose
    /// what a name is shown as, never what it is.
    marks: Memo<Option<Marks>>,
    unsat: Memo<Vec<UnsatisfiedRow>>,
    on_trace: Callback<(String, String)>,
}

impl Panel {
    /// What `name` is shown as: `marked` of it while marking is on, the
    /// name itself otherwise.
    fn shown(&self, name: &str, marked: impl Fn(&Marks) -> String) -> String {
        self.marks
            .with(|m| m.as_ref().map(marked))
            .unwrap_or_else(|| name.to_string())
    }

    /// `name` with whatever any agent substituted, for a place that belongs
    /// to no one agent.
    fn any(&self, name: &str) -> String {
        self.shown(name, |m| m.any(name))
    }
}

fn next_name_id() -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

/// InventoryView component - one simulation's inventory, under the four
/// inner tabs.
#[component]
pub fn InventoryView(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    /// The selection shared by every panel
    state: InventoryState,
    /// The display options shared by every view
    #[prop(into)]
    options: Signal<ViewOptions>,
    /// Opens the Trace tab on a want, as (agent, behavior)
    on_trace: Callback<(String, String)>,
) -> impl IntoView {
    // Only Self and Bundle shape the lists; SuperAgent folding is already in
    // the tracker. Memoized so the SuperAgents switch alone recomputes
    // nothing until the tracker is rebuilt.
    let qopts = Memo::new(move |_| {
        let o = options.get();
        QueryOptions {
            fold_self: o.fold_self,
            bundle: o.bundle,
        }
    });

    let agent_names = Memo::new(move |_| {
        tracker.with(|t| {
            t.as_ref()
                .map(|t| t.get_working_agent_names().into_iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        })
    });
    let behavior_names = Memo::new(move |_| {
        tracker.with(|t| {
            let mut names: Vec<String> = t
                .as_ref()
                .map(|t| t.get_working_behaviors().into_iter().collect())
                .unwrap_or_default();
            names.sort();
            names
        })
    });
    let unsat = Memo::new(move |_| {
        let o = qopts.get();
        tracker.with(|t| {
            t.as_ref()
                .map(|t| query::unsatisfied(t, o))
                .unwrap_or_default()
        })
    });
    let unused = Memo::new(move |_| {
        let o = qopts.get();
        tracker.with(|t| t.as_ref().map(|t| query::unused(t, o)).unwrap_or_default())
    });
    let agent_view = Memo::new(move |_| {
        let agent = state.agent.get()?;
        let o = qopts.get();
        tracker.with(|t| {
            let t = t.as_ref()?;
            Some(match query::agent_provides(t, &agent, o) {
                Some(p) => AgentView::Provides(p),
                None => match t.collective_members(&agent) {
                    Some(members) => AgentView::Members(agent, members),
                    None => AgentView::Absent(agent),
                },
            })
        })
    });
    let behavior_view = Memo::new(move |_| {
        let behavior = state.behavior.get()?;
        let o = qopts.get();
        tracker.with(|t| {
            let t = t.as_ref()?;
            Some(match query::behavior_use(t, &behavior, o) {
                Some(u) => BehaviorView::Use(u),
                None => BehaviorView::Absent(behavior),
            })
        })
    });

    // Read only while marking is on, and once per tracker.
    let marks = Memo::new(move |_| {
        options
            .get()
            .mark_bound
            .then(|| tracker.with(|t| t.as_ref().map(Marks::new)))
            .flatten()
    });

    let panel = Panel {
        state,
        tracker,
        qopts,
        marks,
        unsat,
        on_trace,
    };
    provide_context(panel);

    // A click anywhere but on a name or its popover, or Escape, closes the
    // popover.
    let peek = state.peek;
    let click = window_event_listener(ev::click, move |ev| {
        if peek.get_untracked().is_none() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest(".inv-name-wrap").ok().flatten())
            .is_some();
        if !inside {
            peek.set(None);
        }
    });
    let key = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" && peek.get_untracked().is_some() {
            peek.set(None);
        }
    });
    on_cleanup(move || {
        click.remove();
        key.remove();
    });

    // Search text for the pickers, kept across inner tab switches.
    let agent_query = RwSignal::new(String::new());
    let behavior_query = RwSignal::new(String::new());

    let tab_link = move |tab: InventoryTab, label: Signal<String>| {
        view! {
            <li class="nav-item">
                <a
                    class="nav-link"
                    class:active=move || state.tab.get() == tab
                    href="#"
                    on:click=move |ev| {
                        ev.prevent_default();
                        state.tab.set(tab);
                    }
                >
                    {move || label.get()}
                </a>
            </li>
        }
    };
    let unsat_count = Signal::derive(move || {
        unsat.with(|rows| rows.iter().map(|r| r.agents.len()).sum::<usize>())
    });
    let unused_count = Signal::derive(move || unused.with(|u| u.len()));

    view! {
        <div class="inv-panel">
            <ul class="nav nav-tabs mb-2">
                {tab_link(InventoryTab::Agents, Signal::derive(|| "Agents".to_string()))}
                {tab_link(InventoryTab::Behaviors, Signal::derive(|| "Behaviors".to_string()))}
                {tab_link(
                    InventoryTab::Unsatisfied,
                    Signal::derive(move || format!("Unsatisfied ({})", unsat_count.get())),
                )}
                {tab_link(
                    InventoryTab::Unused,
                    Signal::derive(move || format!("Unused ({})", unused_count.get())),
                )}
            </ul>
            {move || match state.tab.get() {
                InventoryTab::Agents => view! {
                    <div class="trail-layout">
                        <Picker
                            items=agent_names
                            selected=state.agent
                            query=agent_query
                            placeholder="Find an agent"
                            empty="No agents in this simulation."
                            on_pick=Callback::new(move |name| state.select_agent(name))
                        />
                        <div class="trail-main">
                            {move || agents_detail(agent_view.get(), qopts.with(|o| o.fold_self))}
                        </div>
                    </div>
                }
                    .into_any(),
                InventoryTab::Behaviors => view! {
                    <div class="trail-layout">
                        <Picker
                            items=behavior_names
                            selected=state.behavior
                            query=behavior_query
                            placeholder="Find a behavior"
                            label=Callback::new(move |name: String| panel.any(&name))
                            empty="No behaviors in this simulation."
                            on_pick=Callback::new(move |name| state.select_behavior(name))
                        />
                        <div class="trail-main">
                            {move || {
                                behaviors_detail(behavior_view.get(), qopts.with(|o| o.fold_self))
                            }}
                        </div>
                    </div>
                }
                    .into_any(),
                InventoryTab::Unsatisfied => view! {
                    <div class="trail-main">{move || unsatisfied_list(unsat.get())}</div>
                }
                    .into_any(),
                InventoryTab::Unused => view! {
                    <div class="trail-main">{move || unused_list(unused.get())}</div>
                }
                    .into_any(),
            }}
        </div>
    }
}

/// A searchable list of names; a click on one navigates to it.
#[component]
fn Picker(
    #[prop(into)] items: Signal<Vec<String>>,
    selected: RwSignal<Option<String>>,
    query: RwSignal<String>,
    placeholder: &'static str,
    /// What a name is shown as, when not as itself. The search and the
    /// pick are always by the name.
    #[prop(optional)]
    label: Option<Callback<String, String>>,
    empty: &'static str,
    on_pick: Callback<String>,
) -> impl IntoView {
    let shown = Memo::new(move |_| {
        let q = search_key(&query.get());
        items.with(|names| {
            names
                .iter()
                .filter(|n| q.is_empty() || search_key(n).contains(&q))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    view! {
        <div class="trail-wants inv-picker">
            <div class="trail-wants-head">
                <input
                    type="search"
                    class="form-control form-control-sm"
                    placeholder=placeholder
                    aria-label=placeholder
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                />
            </div>
            {move || {
                let names = shown.get();
                if names.is_empty() {
                    let text = if items.with(|i| i.is_empty()) { empty } else { "No match." };
                    return view! {
                        <div class="trail-muted" style="padding: 0.5rem;">{text}</div>
                    }
                        .into_any();
                }
                names
                    .into_iter()
                    .map(|name| {
                        let for_active = name.clone();
                        let for_click = name.clone();
                        let name = match label {
                            Some(label) => label.run(name),
                            None => name,
                        };
                        view! {
                            <button
                                type="button"
                                class="trail-want"
                                class:trail-want-active=move || {
                                    selected.with(|s| s.as_deref() == Some(for_active.as_str()))
                                }
                                on:click=move |_| on_pick.run(for_click.clone())
                            >
                                <span class="trail-want-label">{name}</span>
                            </button>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

/// A name inside a list: a click peeks at it in a small popover.
#[component]
fn Name(
    subject: Subject,
    /// What to show in place of the subject's own name; the subject stays
    /// what a click peeks at
    #[prop(optional)]
    shown: Option<String>,
) -> impl IntoView {
    let panel = expect_context::<Panel>();
    let peek = panel.state.peek;
    let id = next_name_id();
    let open = Memo::new(move |_| peek.get() == Some(id));
    let class = match &subject {
        Subject::Agent(_) => "inv-name inv-name-agent",
        Subject::Behavior(_) => "inv-name",
    };
    // A behavior nobody gave a label for is still marked, with whatever any
    // agent substituted in it.
    let label = {
        let subject = subject.clone();
        move || match (&shown, &subject) {
            (Some(shown), _) => shown.clone(),
            (None, Subject::Agent(n)) => n.clone(),
            (None, Subject::Behavior(n)) => panel.any(n),
        }
    };
    view! {
        <span class="inv-name-wrap">
            <button
                type="button"
                class=class
                title="Click for a summary"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| peek.update(|p| *p = if *p == Some(id) { None } else { Some(id) })
            >
                {label}
            </button>
            <Show when=move || open.get()>
                <Peek subject=subject.clone() />
            </Show>
        </span>
    }
}

/// The popover behind a name: a summary, and Open to navigate.
#[component]
fn Peek(subject: Subject) -> impl IntoView {
    let panel = expect_context::<Panel>();
    let (kind, name) = match &subject {
        Subject::Agent(n) => ("Agent", n.clone()),
        Subject::Behavior(n) => ("Behavior", n.clone()),
    };
    let body = {
        let subject = subject.clone();
        move || match &subject {
            Subject::Agent(name) => agent_peek(panel, name),
            Subject::Behavior(name) => behavior_peek(panel, name),
        }
    };
    let title = {
        let subject = subject.clone();
        move || match &subject {
            Subject::Agent(n) => n.clone(),
            Subject::Behavior(n) => panel.any(n),
        }
    };
    let open = move |_| match &subject {
        Subject::Agent(n) => panel.state.open_agent(n.clone()),
        Subject::Behavior(n) => panel.state.open_behavior(n.clone()),
    };
    let aria = format!("{} {}", kind, name);
    view! {
        <div class="inv-peek" role="dialog" aria-label=aria>
            <div class="inv-peek-head">
                <span class="trail-inspector-label">{kind}</span>
                <button
                    type="button"
                    class="btn-close inv-peek-close"
                    aria-label="Close"
                    on:click=move |_| panel.state.peek.set(None)
                ></button>
            </div>
            <div class="inv-peek-title">{title}</div>
            {body}
            <div class="inv-peek-actions">
                <button type="button" class="btn btn-sm btn-primary" on:click=open>
                    "Open"
                </button>
            </div>
        </div>
    }
}

/// An agent at a glance: how many provides and wants, and which are broken.
fn agent_peek(panel: Panel, name: &str) -> AnyView {
    let o = panel.qopts.get();
    let not_held = panel
        .unsat
        .with(|rows| rows.iter().filter(|r| r.agents.iter().any(|a| a == name)).count());
    let summary = panel.tracker.with(|t| {
        let t = t.as_ref()?;
        Some(match query::agent_provides(t, name, o) {
            Some(p) => Ok((p, t.get_agent_wants(name.to_string()).len())),
            None => Err(t.collective_members(name)),
        })
    });
    match summary {
        None | Some(Err(None)) => {
            view! { <div class="trail-muted">"Not present in this simulation."</div> }.into_any()
        }
        Some(Err(Some(members))) => view! {
            <div class="trail-muted">
                {format!("A SuperAgent of {}, unfolded here.", plural(members.len(), "member", "members"))}
            </div>
        }
            .into_any(),
        Some(Ok((p, wants))) => {
            let broken = p.provides.iter().filter(|r| !r.held).count();
            let held = broken == 0 && not_held == 0;
            let provides = if broken == 0 {
                format!("{}, all held", plural(p.provides.len(), "provide", "provides"))
            } else {
                format!("{}, {} broken", plural(p.provides.len(), "provide", "provides"), broken)
            };
            let patterns = (!p.patterns.is_empty())
                .then(|| plural(p.patterns.len(), "pattern", "patterns"));
            let wants_line = if not_held == 0 {
                format!("{}, all held", plural(wants, "want", "wants"))
            } else {
                format!("{}, {} not held", plural(wants, "want", "wants"), not_held)
            };
            view! {
                <div class="inv-peek-status" class:trail-text-broken=!held>
                    <span class="trail-dot" class:trail-dot-broken=!held></span>
                    {if held { "Held" } else { "Broken" }}
                </div>
                <div>{provides}</div>
                {patterns.map(|p| view! { <div>{format!("plus {}", p)}</div> })}
                <div>{wants_line}</div>
            }
            .into_any()
        }
    }
}

/// A behavior at a glance: who provides it and who uses it.
fn behavior_peek(panel: Panel, name: &str) -> AnyView {
    let o = panel.qopts.get();
    let found = panel
        .tracker
        .with(|t| t.as_ref().and_then(|t| query::behavior_use(t, name, o)));
    let Some(u) = found else {
        return view! { <div class="trail-muted">"Not present in this simulation."</div> }
            .into_any();
    };
    let names = |rows: Vec<Vec<String>>| {
        let mut all: Vec<String> = rows.into_iter().flatten().collect();
        all.sort();
        all.dedup();
        if all.is_empty() {
            "nobody".to_string()
        } else {
            all.join(", ")
        }
    };
    let providers = names(u.providers.iter().map(|r| r.agents.clone()).collect());
    let consumers = names(u.consumers.iter().map(|r| r.agents.clone()).collect());
    let held = u.satisfied;
    view! {
        <div class="inv-peek-status" class:trail-text-broken=!held>
            <span class="trail-dot" class:trail-dot-broken=!held></span>
            {if held { "Held" } else { "Not held" }}
        </div>
        <div class="trail-inspector-label">"Providers"</div>
        <div class="inv-peek-names">{providers}</div>
        <div class="trail-inspector-label">"Consumers"</div>
        <div class="inv-peek-names">{consumers}</div>
    }
    .into_any()
}

/// One or more agents in a row. A bundle shows as "×N noun" and opens to
/// its agents on a click. With `trace`, each agent gets a link to the Trace
/// tab on its want of that behavior.
#[component]
fn AgentsCell(
    agents: Vec<String>,
    noun: &'static str,
    #[prop(optional)] trace: Option<String>,
) -> impl IntoView {
    if agents.len() == 1 {
        let agent = agents[0].clone();
        return view! {
            <Name subject=Subject::Agent(agent.clone()) />
            {trace.map(|behavior| view! { <TraceLink agent=agent behavior=behavior /> })}
        }
        .into_any();
    }
    let (open, set_open) = signal(false);
    let count = agents.len();
    view! {
        <span class="inv-bundle">
            <button
                type="button"
                class="inv-bundle-toggle"
                aria-expanded=move || open.get().to_string()
                title=move || if open.get() { "Hide the agents" } else { "List the agents" }
                on:click=move |_| set_open.update(|o| *o = !*o)
            >
                {format!("\u{00d7}{} {}", count, noun)}
            </button>
            <Show when=move || open.get()>
                <ul class="inv-bundle-list">
                    {agents
                        .iter()
                        .map(|a| {
                            let trace = trace.clone();
                            let agent = a.clone();
                            view! {
                                <li>
                                    <Name subject=Subject::Agent(a.clone()) />
                                    {trace.map(|behavior| view! { <TraceLink agent=agent behavior=behavior /> })}
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            </Show>
        </span>
    }
    .into_any()
}

/// Opens the Trace tab on one want.
#[component]
fn TraceLink(agent: String, behavior: String) -> impl IntoView {
    let panel = expect_context::<Panel>();
    let title = format!("Trace {} wanting {}", agent, behavior);
    view! {
        <button
            type="button"
            class="btn btn-link btn-sm p-0 inv-trace"
            title=title
            on:click=move |_| panel.on_trace.run((agent.clone(), behavior.clone()))
        >
            "trace"
        </button>
    }
}

/// A condition, marked held or broken.
///
/// `shown` is the text to display when it differs from the behavior's name.
fn condition_chip(c: ConditionStatus, shown: Option<String>) -> impl IntoView {
    let held = c.held;
    let shown = shown.unwrap_or_else(|| c.behavior.clone());
    view! {
        <span class="inv-cond" class:inv-cond-broken=!held>
            <Name subject=Subject::Behavior(c.behavior) shown=shown />
            {if held { " \u{2713}" } else { " \u{2717}" }}
        </span>
    }
}

/// A promise's conditions, each shown as `shown` says.
fn conditions_view(
    conditions: Vec<ConditionStatus>,
    shown: impl Fn(&str) -> String,
) -> Option<AnyView> {
    (!conditions.is_empty()).then(|| {
        view! {
            <span class="trail-muted">"if"</span>
            {conditions
                .into_iter()
                .map(|c| {
                    let label = shown(&c.behavior);
                    condition_chip(c, Some(label))
                })
                .collect_view()}
        }
        .into_any()
    })
}

fn empty_note(text: String) -> AnyView {
    view! { <div class="trail-empty">{text}</div> }.into_any()
}

fn status_strip(held: bool, text: String) -> impl IntoView {
    view! {
        <div class=if held {
            "trail-status trail-status-held"
        } else {
            "trail-status trail-status-broken"
        }>{text}</div>
    }
}

/// The Agents tab's right side.
fn agents_detail(view: Option<AgentView>, fold_self: bool) -> AnyView {
    let panel = expect_context::<Panel>();
    match view {
        None => empty_note("Pick an agent on the left to see what it provides.".to_string()),
        Some(AgentView::Absent(name)) => {
            empty_note(format!("{} is not present in this simulation.", name))
        }
        Some(AgentView::Members(name, members)) => view! {
            <div class="trail-status inv-status-plain">
                {format!("{} is a SuperAgent. With SuperAgents unfolded its members stand on their own:", name)}
            </div>
            <div class="inv-section">
                {if members.is_empty() {
                    view! { <div class="trail-muted">"It has no members."</div> }.into_any()
                } else {
                    members
                        .into_iter()
                        .map(|m| {
                            let pick = m.clone();
                            view! {
                                <button
                                    type="button"
                                    class="trail-want"
                                    on:click=move |_| panel.state.select_agent(pick.clone())
                                >
                                    <span class="trail-want-label">{m}</span>
                                </button>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </div>
        }
        .into_any(),
        Some(AgentView::Provides(p)) => {
            let broken = p.provides.iter().filter(|r| !r.held).count();
            let count = plural(p.provides.len(), "provide", "provides");
            let summary = if p.provides.is_empty() {
                format!(
                    "{} provides nothing{}.",
                    p.agent,
                    if fold_self { " once self-promises are folded" } else { "" }
                )
            } else if broken == 0 {
                format!("{}: {}, all held.", p.agent, count)
            } else {
                format!("{}: {}, {} broken.", p.agent, count, broken)
            };
            // Names as this agent has them, marked when asked for: a promise
            // as it makes it, a condition as it needs it.
            let agent = p.agent.clone();
            let promised = {
                let agent = agent.clone();
                move |name: &str| panel.shown(name, |m| m.promise(&agent, name))
            };
            let needed = move |name: &str| panel.shown(name, |m| m.need(&agent, name));
            let rows = p
                .provides
                .into_iter()
                .map(|r| {
                    let held = r.held;
                    let label = promised(&r.behavior);
                    let conditions = (!r.conditions.is_empty()).then(|| {
                        let chips = r
                            .conditions
                            .into_iter()
                            .map(|c| {
                                let label = needed(&c.behavior);
                                condition_chip(c, Some(label))
                            })
                            .collect_view();
                        view! {
                            <span class="trail-muted">"if"</span>
                            {chips}
                        }
                    });
                    view! {
                        <div class="inv-row">
                            <span class="trail-dot" class:trail-dot-broken=!held></span>
                            <Name subject=Subject::Behavior(r.behavior) shown=label />
                            {conditions}
                        </div>
                    }
                })
                .collect_view();
            let patterns = (!p.patterns.is_empty()).then(|| {
                let rows = p
                    .patterns
                    .into_iter()
                    .map(|r| {
                        let conditions = (!r.conditions.is_empty()).then(|| {
                            format!(
                                "if {}",
                                r.conditions
                                    .iter()
                                    .map(|c| needed(c))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        });
                        let pattern = promised(&r.pattern);
                        view! {
                            <div class="inv-row">
                                <code class="inv-pattern">{pattern}</code>
                                <span class="inv-tag">"pattern"</span>
                                {conditions.map(|c| view! { <span class="trail-muted inv-mono">{c}</span> })}
                            </div>
                        }
                    })
                    .collect_view();
                view! {
                    <div class="inv-section">
                        <div class="trail-inspector-label">"Patterns"</div>
                        {rows}
                    </div>
                }
            });
            view! {
                {status_strip(broken == 0, summary)}
                <div class="inv-section">{rows}</div>
                {patterns}
            }
            .into_any()
        }
    }
}

/// The Behaviors tab's right side.
fn behaviors_detail(view: Option<BehaviorView>, fold_self: bool) -> AnyView {
    let panel = expect_context::<Panel>();
    match view {
        None => empty_note("Pick a behavior on the left to see who provides and uses it.".to_string()),
        Some(BehaviorView::Absent(name)) => {
            empty_note(format!("{} is not present in this simulation.", name))
        }
        Some(BehaviorView::Use(u)) => {
            // The behavior belongs to no one agent here: marked with what
            // any of them substituted.
            let title = panel.any(&u.behavior);
            if fold_self && u.providers.is_empty() && u.consumers.is_empty() {
                return empty_note(format!(
                    "{} is only a self-promise, folded away.",
                    title
                ));
            }
            let summary = format!(
                "{}: {}.",
                title,
                if u.satisfied { "held" } else { "not held" }
            );
            let provider_count = u.providers.iter().map(|r| r.agents.len()).sum::<usize>();
            let consumer_count = u.consumers.iter().map(|r| r.agents.len()).sum::<usize>();
            let providers = if u.providers.is_empty() {
                view! { <div class="trail-muted inv-row">"Nobody provides it."</div> }.into_any()
            } else {
                u.providers
                    .into_iter()
                    .map(|r| {
                        let held = r.held;
                        // As the row's agents wrote the promise, and need
                        // its conditions.
                        let pattern = r
                            .pattern
                            .map(|p| panel.shown(&p, |m| m.promise_by(&r.agents, &p)));
                        let conditions = conditions_view(r.conditions, |c| {
                            panel.shown(c, |m| m.need_by(&r.agents, c))
                        });
                        view! {
                            <div class="inv-row">
                                <span class="trail-dot" class:trail-dot-broken=!held></span>
                                <AgentsCell agents=r.agents noun="providers" />
                                {pattern.map(|p| view! {
                                    <span class="trail-muted">"via"</span>
                                    <code class="inv-pattern">{p}</code>
                                    <span class="inv-tag">"pattern"</span>
                                })}
                                {conditions}
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            };
            let consumers = if u.consumers.is_empty() {
                view! { <div class="trail-muted inv-row">"Nobody uses it."</div> }.into_any()
            } else {
                u.consumers
                    .into_iter()
                    .map(|r| {
                        let how = match r.kind {
                            ConsumerKind::Want => view! {
                                <span class="trail-muted">"wants it"</span>
                            }
                            .into_any(),
                            ConsumerKind::Condition { provide } => {
                                let provide =
                                    panel.shown(&provide, |m| m.promise_by(&r.agents, &provide));
                                view! {
                                    <span class="trail-muted">"needs it for its promise"</span>
                                    <code class="inv-pattern">{provide}</code>
                                }
                                .into_any()
                            }
                        };
                        view! {
                            <div class="inv-row">
                                <AgentsCell agents=r.agents noun="consumers" />
                                {how}
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            };
            view! {
                {status_strip(u.satisfied, summary)}
                <div class="inv-section">
                    <div class="trail-inspector-label">{format!("Providers ({})", provider_count)}</div>
                    {providers}
                </div>
                <div class="inv-section">
                    <div class="trail-inspector-label">{format!("Consumers ({})", consumer_count)}</div>
                    {consumers}
                </div>
            }
            .into_any()
        }
    }
}

/// The Unsatisfied tab.
fn unsatisfied_list(rows: Vec<UnsatisfiedRow>) -> AnyView {
    let panel = expect_context::<Panel>();
    if rows.is_empty() {
        return status_strip(true, "Every want is held.".to_string()).into_any();
    }
    let wants = rows.iter().map(|r| r.agents.len()).sum::<usize>();
    let rows = rows
        .into_iter()
        .map(|r| {
            // The want as the row's agents wrote it.
            let wanted = panel.shown(&r.behavior, |m| m.need_by(&r.agents, &r.behavior));
            let wanters = r.agents.clone();
            let causes = r
                .causes
                .into_iter()
                .map(|c| {
                    let why = match c.kind {
                        CauseKind::Unprovided => "nobody provides it",
                        CauseKind::Cycle => "only promised through a loop",
                    };
                    // The cause as whoever needs it has it: the promise it
                    // is a condition of, or the wanters themselves.
                    let cause = panel.shown(&c.behavior, |m| match &c.needed_by {
                        Some(site) => m.need(&site.agent, &c.behavior),
                        None => m.need_by(&wanters, &c.behavior),
                    });
                    let site = c.needed_by.map(|s| {
                        let provide = panel.shown(&s.provide, |m| m.kept(&s.agent, &s.provide));
                        view! {
                            <span class="trail-muted">"needed by"</span>
                            <Name subject=Subject::Agent(s.agent) />
                            <span class="trail-muted">"for"</span>
                            <code class="inv-pattern">{provide}</code>
                        }
                    });
                    view! {
                        <div class="inv-cause">
                            <span class="inv-cond inv-cond-broken">
                                <Name subject=Subject::Behavior(c.behavior) shown=cause />
                            </span>
                            <span class="trail-muted">{why}</span>
                            {site}
                        </div>
                    }
                })
                .collect_view();
            view! {
                <div class="inv-row inv-row-block">
                    <div class="inv-row-line">
                        <span class="trail-dot trail-dot-broken"></span>
                        <AgentsCell agents=r.agents noun="agents" trace=r.behavior.clone() />
                        <span class="trail-muted">"wants"</span>
                        <Name subject=Subject::Behavior(r.behavior) shown=wanted />
                    </div>
                    {causes}
                </div>
            }
        })
        .collect_view();
    view! {
        {status_strip(false, format!("{} not held.", plural(wants, "want", "wants")))}
        <div class="inv-section">{rows}</div>
    }
    .into_any()
}

/// The Unused tab.
fn unused_list(groups: Vec<UnusedBehavior>) -> AnyView {
    let panel = expect_context::<Panel>();
    if groups.is_empty() {
        return status_strip(true, "Every promise is used.".to_string()).into_any();
    }
    let count = groups.len();
    let groups = groups
        .into_iter()
        .map(|g| {
            // The promise as the agents making it wrote it.
            let makers: Vec<String> = g
                .providers
                .iter()
                .flat_map(|p| p.agents.iter().cloned())
                .collect();
            let promised = panel.shown(&g.behavior, |m| m.promise_by(&makers, &g.behavior));
            let head = if g.pattern {
                view! {
                    <code class="inv-pattern">{promised}</code>
                    <span class="inv-tag">"pattern"</span>
                }
                .into_any()
            } else {
                view! { <Name subject=Subject::Behavior(g.behavior) shown=promised /> }.into_any()
            };
            let providers = g
                .providers
                .into_iter()
                .map(|p| {
                    let conditions = (!p.conditions.is_empty()).then(|| {
                        format!(
                            "if {}",
                            p.conditions
                                .iter()
                                .map(|c| panel.shown(c, |m| m.need_by(&p.agents, c)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    });
                    view! {
                        <div class="inv-row-line inv-indent">
                            <span class="trail-muted">"by"</span>
                            <AgentsCell agents=p.agents noun="providers" />
                            {conditions.map(|c| view! { <span class="trail-muted inv-mono">{c}</span> })}
                        </div>
                    }
                })
                .collect_view();
            view! {
                <div class="inv-row inv-row-block">
                    <div class="inv-row-line">{head}</div>
                    {providers}
                </div>
            }
        })
        .collect_view();
    view! {
        <div class="trail-status inv-status-plain">
            {format!("{} nobody uses.", plural(count, "promised behavior", "promised behaviors"))}
        </div>
        <div class="inv-section">{groups}</div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folding(member: &str) -> Vec<String> {
        match member {
            "web-1" => vec!["web".to_string(), "zone".to_string()],
            _ => vec![],
        }
    }

    #[test]
    fn folding_moves_a_member_to_its_first_collective_and_remembers_it() {
        let (sel, mem) = follow_fold(Some("web-1".into()), None, true, folding);
        assert_eq!(sel.as_deref(), Some("web"));
        assert_eq!(mem, Some(("web-1".to_string(), "web".to_string())));
    }

    #[test]
    fn folding_leaves_an_unfolded_agent_alone() {
        let (sel, mem) = follow_fold(Some("db".into()), None, true, folding);
        assert_eq!(sel.as_deref(), Some("db"));
        assert_eq!(mem, None);
    }

    #[test]
    fn unfolding_returns_to_the_remembered_member() {
        let memory = Some(("web-1".to_string(), "web".to_string()));
        let (sel, mem) = follow_fold(Some("web".into()), memory, false, folding);
        assert_eq!(sel.as_deref(), Some("web-1"));
        assert_eq!(mem, None);
    }

    #[test]
    fn unfolding_keeps_a_collective_reached_another_way() {
        let memory = Some(("web-1".to_string(), "web".to_string()));
        let (sel, mem) = follow_fold(Some("zone".into()), memory, false, folding);
        assert_eq!(sel.as_deref(), Some("zone"));
        assert_eq!(mem, None);
        let (sel, _) = follow_fold(Some("web".into()), None, false, folding);
        assert_eq!(sel.as_deref(), Some("web"));
    }

    #[test]
    fn nothing_selected_stays_unselected() {
        assert_eq!(follow_fold(None, None, true, folding), (None, None));
    }
}
