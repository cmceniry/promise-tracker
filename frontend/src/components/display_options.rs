use leptos::prelude::*;
use promise_tracker::Tracker;

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
    /// Offers that are not kept are folded away, with everything they rest
    /// on, where another offer already keeps the same behavior. A behavior
    /// nobody keeps still shows every offer. Display only.
    pub fold_alternatives: bool,
    /// Values a variable was replaced with are wrapped in `<>` wherever a
    /// behavior is named. Display only.
    pub mark_bound: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            collectives: true,
            bundle: true,
            fold_self: false,
            fold_alternatives: false,
            mark_bound: false,
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
        // Said only when on, like the mark below.
        if self.fold_alternatives {
            folded.push("unneeded alternatives folded");
        }
        let folded = if folded.is_empty() {
            "nothing folded".to_string()
        } else {
            folded.join(", ")
        };
        // Said only when on, so an export without it reads as it always has.
        if self.mark_bound {
            format!("{}, bound values marked", folded)
        } else {
            folded
        }
    }

    /// A want as a view's heading names it: while bound values are marked,
    /// as `agent` wrote it, with what a substitution filled in wrapped in
    /// `<>`; otherwise, or with no tracker to ask, the want itself.
    ///
    /// For titles and labels only. A file name, an option's value and every
    /// lookup keep the plain want.
    pub fn want_label(&self, tracker: Option<&Tracker>, agent: &str, want: &str) -> String {
        match tracker {
            Some(tracker) if self.mark_bound => tracker.marked_want(agent, want),
            _ => want.to_string(),
        }
    }
}

/// The subtitle of a Detailed view's frame and export: whose want it shows,
/// how the view is folded, and when it was made. `behavior` is the want as
/// the heading should name it ([`ViewOptions::want_label`]).
pub fn focus_subtitle(component: &str, behavior: &str, folded: &str, generated: &str) -> String {
    format!(
        "{} -> {} - {} - generated {}",
        component, behavior, folded, generated
    )
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

/// The display options, applying to every view below. Switches of equal
/// standing, on one line.
#[component]
pub fn DisplayOptions(
    options: ReadSignal<ViewOptions>,
    set_options: WriteSignal<ViewOptions>,
) -> impl IntoView {
    let collectives = Signal::derive(move || options.get().collectives);
    let bundle = Signal::derive(move || options.get().bundle);
    let fold_self = Signal::derive(move || options.get().fold_self);
    let fold_alternatives = Signal::derive(move || options.get().fold_alternatives);
    let mark_bound = Signal::derive(move || options.get().mark_bound);

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
            <OptionToggle
                on=fold_alternatives
                label="Unneeded alternatives"
                about="Fold away offers that are not kept, and everything they rest on, where another offer already keeps the same behavior; a behavior nobody keeps still shows every offer"
                on_click=Callback::new(move |_| set_options.update(|o| o.fold_alternatives = !o.fold_alternatives))
            >
                // Two ways to the same point: one kept, one dashed and struck out.
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 16 16">
                    <path d="M1 8 C5 8 6 4 14 4"></path>
                    <path d="M1 8 C5 8 6 12 14 12" stroke-dasharray="2 1.5"></path>
                    <path d="M9 9.5 12 13.5 M12 9.5 9 13.5"></path>
                </svg>
            </OptionToggle>
            <span style="font-weight: bold;">"Mark:"</span>
            <OptionToggle
                on=mark_bound
                label="Bound values"
                about="Wrap values a variable was replaced with in <>, such as an agent's own name where it wrote {{!self}} or what a {{variable}} matched, wherever a behavior is named"
                on_click=Callback::new(move |_| set_options.update(|o| o.mark_bound = !o.mark_bound))
            >
                // Angle brackets around a value.
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 16 16">
                    <path d="M5 3 1 8 5 13 M11 3 15 8 11 13"></path>
                    <circle cx="8" cy="8" r="1.5" fill="currentColor" stroke="none"></circle>
                </svg>
            </OptionToggle>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use promise_tracker::components::{Agent, Behavior};

    fn tracker() -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(
            Agent::build("a1")
                .with_wants(vec![
                    Behavior::build("{{!self}} reads r1"),
                    Behavior::build("a2 reads r1"),
                ])
                .with_provides(vec![Behavior::build("{{who}} reads r1")]),
        );
        t
    }

    #[test]
    fn a_heading_marks_the_want_only_while_marking_is_on() {
        let t = tracker();
        let off = ViewOptions::default();
        let on = ViewOptions {
            mark_bound: true,
            ..off
        };
        assert_eq!(off.want_label(Some(&t), "a1", "a1 reads r1"), "a1 reads r1");
        assert_eq!(on.want_label(Some(&t), "a1", "a1 reads r1"), "<a1> reads r1");
        // Written out: plain either way, whatever the agent's promise does
        // with the same text.
        assert_eq!(on.want_label(Some(&t), "a1", "a2 reads r1"), "a2 reads r1");
        // Nothing chosen yet, or nothing to ask.
        assert_eq!(on.want_label(Some(&t), "---", "---"), "---");
        assert_eq!(on.want_label(None, "a1", "a1 reads r1"), "a1 reads r1");
    }

    #[test]
    fn the_subtitle_names_the_want_as_it_is_given() {
        let t = tracker();
        let off = ViewOptions::default();
        let on = ViewOptions {
            mark_bound: true,
            ..off
        };
        let subtitle = |o: ViewOptions| {
            focus_subtitle(
                "a1",
                &o.want_label(Some(&t), "a1", "a1 reads r1"),
                &o.describe(),
                "now",
            )
        };
        // Off, the text it always was.
        assert_eq!(
            subtitle(off),
            format!("{} -> {} - {} - generated {}", "a1", "a1 reads r1", off.describe(), "now")
        );
        assert_eq!(
            subtitle(off),
            "a1 -> a1 reads r1 - SuperAgents folded, parallel promises bundled - generated now"
        );
        assert_eq!(
            subtitle(on),
            "a1 -> <a1> reads r1 - SuperAgents folded, parallel promises bundled, \
             bound values marked - generated now"
        );
    }

    #[test]
    fn describe_names_alternative_folding_only_when_on() {
        let off = ViewOptions::default();
        assert!(!off.fold_alternatives);
        assert_eq!(off.describe(), "SuperAgents folded, parallel promises bundled");
        let on = ViewOptions {
            fold_alternatives: true,
            ..off
        };
        assert_eq!(
            on.describe(),
            "SuperAgents folded, parallel promises bundled, unneeded alternatives folded"
        );
        // after the other folds, before the mark
        let all = ViewOptions {
            fold_self: true,
            mark_bound: true,
            ..on
        };
        assert_eq!(
            all.describe(),
            "SuperAgents folded, parallel promises bundled, self-fulfilling promises folded, \
             unneeded alternatives folded, bound values marked"
        );
        // alone, it is still something folded
        let only = ViewOptions {
            collectives: false,
            bundle: false,
            ..on
        };
        assert_eq!(only.describe(), "unneeded alternatives folded");
    }
}
