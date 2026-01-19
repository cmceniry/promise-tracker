use leptos::prelude::*;
use promise_tracker::resolve::{Offer, Resolution};
use promise_tracker::Tracker;

/// Renders a single offer as a list item with optional nested conditions
fn render_offer(offer: &Offer, is_satisfied: bool) -> impl IntoView {
    let agent_name = offer.get_agent_name().to_string();
    let conditions = offer.get_resolved_conditions();

    let css_class = if is_satisfied {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    let label = if is_satisfied {
        format!("OPTION: {}", agent_name)
    } else {
        format!("POSSIBLE: {}", agent_name)
    };

    if conditions.is_empty() {
        // No conditions - just render the label
        view! {
            <li class=css_class>{label}</li>
        }
        .into_any()
    } else {
        // Has conditions - render with nested children
        let children: Vec<_> = conditions.iter().map(render_resolution_li).collect();

        view! {
            <li class=css_class>
                {label}
                <ul class="contract-text-list">{children}</ul>
            </li>
        }
        .into_any()
    }
}

/// Renders a Resolution as a list item with its satisfying/unsatisfying offers
fn render_resolution_li(resolution: &Resolution) -> impl IntoView {
    let behavior_name = resolution.get_behavior_name().to_string();
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return view! {
            <li class="contract-text-possible">{format!("{} UNSATISFIED", behavior_name)}</li>
        }
        .into_any();
    }

    // Collect all options (satisfied first, then unsatisfied)
    let mut options: Vec<AnyView> = Vec::new();

    for offer in satisfying.iter() {
        options.push(render_offer(offer, true).into_any());
    }

    for offer in unsatisfying.iter() {
        // Check if the offer has no resolved conditions - this is an error case
        let conditions = offer.get_resolved_conditions();
        if conditions.is_empty() {
            options.push(
                view! {
                    <li class="contract-text-error">{format!("ERROR: {}", offer.get_agent_name())}</li>
                }
                .into_any(),
            );
        } else {
            options.push(render_offer(offer, false).into_any());
        }
    }

    // Determine the CSS class based on whether there are any satisfying offers
    let contract_class = if !satisfying.is_empty() {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    view! {
        <li class=contract_class>
            {behavior_name}
            <ul class="contract-text-list">{options}</ul>
        </li>
    }
    .into_any()
}

/// Renders the root resolution (component --> behavior) as a list item
fn render_contract_text(component: &str, resolution: &Resolution) -> impl IntoView {
    let behavior_name = resolution.get_behavior_name().to_string();
    let satisfying = resolution.get_satisfying_offers();
    let unsatisfying = resolution.get_unsatisfying_offers();

    // If no offers at all, render as unsatisfied
    if satisfying.is_empty() && unsatisfying.is_empty() {
        return view! {
            <li class="contract-text-possible">
                {format!("{} --> {} UNSATISFIED", component, behavior_name)}
            </li>
        }
        .into_any();
    }

    // Collect all options (satisfied first, then unsatisfied)
    let mut options: Vec<AnyView> = Vec::new();

    for offer in satisfying.iter() {
        options.push(render_offer(offer, true).into_any());
    }

    for offer in unsatisfying.iter() {
        let conditions = offer.get_resolved_conditions();
        if conditions.is_empty() {
            options.push(
                view! {
                    <li class="contract-text-error">{format!("ERROR: {}", offer.get_agent_name())}</li>
                }
                .into_any(),
            );
        } else {
            options.push(render_offer(offer, false).into_any());
        }
    }

    // Determine the CSS class based on whether there are any satisfying offers
    let contract_class = if !satisfying.is_empty() {
        "contract-text-option"
    } else {
        "contract-text-possible"
    };

    view! {
        <li class=contract_class>
            {format!("{} --> {}", component, behavior_name)}
            <ul class="contract-text-list">{options}</ul>
        </li>
    }
    .into_any()
}

/// Displays promise resolution as hierarchical text/list view.
#[component]
pub fn ContractText(
    #[prop(into)] tracker: Signal<Option<Tracker>>,
    selected_component: ReadSignal<String>,
    selected_behavior: ReadSignal<String>,
) -> impl IntoView {
    let content = move || {
        let tracker_opt = tracker.get();
        let component = selected_component.get();
        let behavior = selected_behavior.get();

        // Handle edge cases with placeholder messages
        let Some(pt) = tracker_opt else {
            return view! {
                <div class="text-muted p-3">"No tracker available"</div>
            }
            .into_any();
        };

        if pt.is_empty() {
            return view! {
                <div class="text-muted p-3">"Add components to this simulation"</div>
            }
            .into_any();
        }

        if component == "---" {
            return view! {
                <div class="text-muted p-3">"Select a component"</div>
            }
            .into_any();
        }

        if !pt.has_agent(component.clone()) {
            return view! {
                <div class="text-muted p-3">"Select a component in this simulation"</div>
            }
            .into_any();
        }

        if pt.get_agent_wants(component.clone()).is_empty() {
            return view! {
                <div class="text-muted p-3">"Select a component with wants"</div>
            }
            .into_any();
        }

        if behavior == "---" {
            return view! {
                <div class="text-muted p-3">"Select a behavior"</div>
            }
            .into_any();
        }

        if !pt.has_behavior(behavior.clone()) {
            return view! {
                <div class="text-muted p-3">"Select a valid behavior"</div>
            }
            .into_any();
        }

        // Resolve the behavior and render the result
        let resolution = pt.resolve(&behavior);
        let contract_text = render_contract_text(&component, &resolution);

        view! {
            <div class="card">
                <div class="card-body contract-text-card">
                    <ul class="contract-text-list">{contract_text}</ul>
                </div>
            </div>
        }
        .into_any()
    };

    view! { <div class="contract-text">{content}</div> }
}
