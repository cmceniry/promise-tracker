use leptos::prelude::*;

/// How every view reads the contract. One value, so the overview, the
/// detailed views and the trace never disagree about what is folded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewOptions {
    /// Agents stand folded into their SuperAgent. Off, the tracker is built
    /// with every member as an agent of its own.
    pub collectives: bool,
    /// Promises between the same two agents, in the same direction, are
    /// shown as one.
    pub bundle: bool,
    /// Promises an agent makes to itself are folded away.
    pub fold_self: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            collectives: true,
            bundle: true,
            fold_self: false,
        }
    }
}

impl ViewOptions {
    /// What is folded, for an export's subtitle.
    pub fn describe(&self) -> String {
        let mut folded: Vec<&str> = vec![];
        if self.collectives {
            folded.push("SuperAgents folded");
        }
        if self.bundle {
            folded.push("parallel promises bundled");
        }
        if self.fold_self {
            folded.push("self-fulfilling promises folded");
        }
        if folded.is_empty() {
            "nothing folded".to_string()
        } else {
            folded.join(", ")
        }
    }
}

/// One toggle: an icon and a short name, with its meaning and state in the
/// tooltip.
#[component]
fn OptionToggle(
    #[prop(into)] on: Signal<bool>,
    label: &'static str,
    /// What the option does, for the tooltip
    about: &'static str,
    on_click: Callback<()>,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class=move || {
                if on.get() {
                    "btn btn-sm btn-primary view-option"
                } else {
                    "btn btn-sm btn-outline-secondary view-option"
                }
            }
            aria-pressed=move || if on.get() { "true" } else { "false" }
            title=move || format!("{} ({}; click to turn {})", about,
                if on.get() { "on" } else { "off" },
                if on.get() { "off" } else { "on" })
            on:click=move |_| on_click.run(())
        >
            {children()}
            <span>{label}</span>
        </button>
    }
}

/// The display options, applying to every view below. Three switches of
/// equal standing, on one line.
#[component]
pub fn DisplayOptions(
    options: ReadSignal<ViewOptions>,
    set_options: WriteSignal<ViewOptions>,
) -> impl IntoView {
    let collectives = Signal::derive(move || options.get().collectives);
    let bundle = Signal::derive(move || options.get().bundle);
    let fold_self = Signal::derive(move || options.get().fold_self);

    view! {
        <div class="view-options">
            <span style="font-weight: bold;">"Fold:"</span>
            <OptionToggle
                on=collectives
                label="SuperAgents"
                about="Fold agents into their SuperAgent; off, every member is an agent of its own"
                on_click=Callback::new(move |_| set_options.update(|o| o.collectives = !o.collectives))
            >
                // A box holding two: members inside their collective.
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                    <rect x="1" y="2" width="14" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"></rect>
                    <rect x="4" y="5" width="3" height="6" rx="0.5"></rect>
                    <rect x="9" y="5" width="3" height="6" rx="0.5"></rect>
                </svg>
            </OptionToggle>
            <OptionToggle
                on=bundle
                label="Bundle"
                about="Show promises between the same two agents, in the same direction, as one"
                on_click=Callback::new(move |_| set_options.update(|o| o.bundle = !o.bundle))
            >
                // Three lines drawn together into one.
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 16 16">
                    <path d="M1 3 C5 3 6 8 9 8 M1 8 H9 M1 13 C5 13 6 8 9 8"></path>
                    <path d="M9 8 H15" stroke-width="3"></path>
                </svg>
            </OptionToggle>
            <OptionToggle
                on=fold_self
                label="Self promises"
                about="Fold away promises an agent makes to itself"
                on_click=Callback::new(move |_| set_options.update(|o| o.fold_self = !o.fold_self))
            >
                // A loop: the promise comes back to the agent that made it.
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" viewBox="0 0 16 16">
                    <path d="M8 3a5 5 0 1 0 5 5" fill="none" stroke="currentColor" stroke-width="1.5"></path>
                    <path d="M8 0.5 5 3 8 5.5z"></path>
                </svg>
            </OptionToggle>
        </div>
    }
}
