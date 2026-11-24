use clap::Parser;
use promise_tracker::query::{self, Cause, CauseKind, QueryOptions};
use promise_tracker::resolve::Resolution;
use promise_tracker::{HoldsForAny, Tracker};
use std::collections::BTreeSet;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show single line outputs per provides/conditions
    #[clap(short, long)]
    compressed: bool,

    /// Show SuperAgent members as agents of their own
    #[clap(long)]
    unfold: bool,

    /// One line per unsatisfied want with its leaf causes, instead of every
    /// want's resolution
    #[clap(short, long)]
    summary: bool,

    /// Collapse unsatisfied wants that differ only by agent (implies --summary)
    #[clap(long)]
    bundle: bool,

    /// Leave out offers that are not kept, and everything they rest on,
    /// where another offer already keeps the same behavior; a behavior
    /// nobody keeps still shows every offer. Changes nothing
    /// with --summary, which lists only wants nobody keeps
    #[clap(long)]
    fold_alternatives: bool,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    if parameters.summary || parameters.bundle {
        let options = QueryOptions {
            fold_self: false,
            bundle: parameters.bundle,
            // The summary lists only wants nobody keeps, each by its leaf
            // causes: there is no kept behavior in it to fold around.
            fold_alternatives: false,
        };
        for row in query::unsatisfied(&tracker, options) {
            println!(
                "{} {} {}: {}",
                cli::agent_list(&row.agents),
                if row.agents.len() > 1 {
                    "want"
                } else {
                    "wants"
                },
                row.behavior,
                causes(&row.causes)
            );
        }
        return;
    }
    for r in resolutions(&tracker, parameters.fold_alternatives) {
        if parameters.compressed {
            for line in r.to_colorized_compressed_strings() {
                println!("{}", line);
            }
        } else {
            for line in r.to_colorized_strings() {
                println!("{}", line);
            }
        }
    }
}

/// Every want's resolution, one per distinct want, by want. With
/// `fold_alternatives`, each without the offers that fail to keep a behavior
/// another offer already keeps.
fn resolutions(tracker: &Tracker, fold_alternatives: bool) -> Vec<Resolution> {
    let mut wants: BTreeSet<String> = BTreeSet::new();
    for agent_name in tracker.get_working_agent_names() {
        wants.extend(tracker.get_agent_wants(agent_name.clone()));
    }
    wants
        .iter()
        .map(|want| {
            let resolution = tracker.resolve(want);
            if fold_alternatives {
                resolution.without_unneeded_alternatives()
            } else {
                resolution
            }
        })
        .collect()
}

/// Each cause as `c2 is not provided (condition of c1 by q, r)`: causes that
/// differ only by the agent making the promise share a line.
fn causes(causes: &[Cause]) -> String {
    let mut grouped: Vec<(&str, CauseKind, Option<&str>, Vec<&str>)> = vec![];
    for cause in causes {
        let provide = cause.needed_by.as_ref().map(|s| s.provide.as_str());
        let agent = cause.needed_by.as_ref().map(|s| s.agent.as_str());
        match grouped
            .iter_mut()
            .find(|(b, k, p, _)| *b == cause.behavior && *k == cause.kind && *p == provide)
        {
            Some((_, _, _, agents)) => agents.extend(agent),
            None => grouped.push((
                &cause.behavior,
                cause.kind.clone(),
                provide,
                agent.into_iter().collect(),
            )),
        }
    }
    grouped
        .into_iter()
        .map(|(behavior, kind, provide, agents)| {
            let what = match kind {
                // An open variable only finds behaviors promised outright,
                // which is worth saying where it found nothing.
                CauseKind::Unprovided if behavior.contains("{{?") => String::from(
                    "matches nothing promised outright (parameterized promises are not considered)",
                ),
                CauseKind::Unprovided => String::from("is not provided"),
                CauseKind::Cycle => String::from("is only provided through a dependency loop"),
                CauseKind::Unfound {
                    variables,
                    holds_for_any,
                } => unfound(&variables, &holds_for_any),
            };
            match provide {
                None => format!("{} {}", behavior, what),
                Some(provide) => format!(
                    "{} {} (condition of {} by {})",
                    behavior,
                    what,
                    provide,
                    agents.join(", ")
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// `finds no value for `kc``, and when only promises holding for any value
/// answered, which ones.
fn unfound(variables: &[String], holds_for_any: &[HoldsForAny]) -> String {
    let variables: Vec<String> = variables.iter().map(|v| format!("`{}`", v)).collect();
    let mut what = format!("finds no value for {}", variables.join(", "));
    if !holds_for_any.is_empty() {
        let holders: Vec<String> = holds_for_any
            .iter()
            .map(|h| format!("{}'s `{}`", h.agent, h.promise))
            .collect();
        what.push_str(&format!(
            ": only {} answered, holding for any value",
            holders.join(" and ")
        ));
    }
    what
}

#[cfg(test)]
mod tests {
    use super::*;
    use promise_tracker::components::{Agent, Behavior};
    use promise_tracker::resolve::Offer;

    fn bc(name: &str, conditions: &[&str]) -> Behavior {
        Behavior::build(name).with_conditions(conditions.iter().map(|c| c.to_string()).collect())
    }

    /// `x` and `y` both want `b1`, kept by `good` and offered brokenly by
    /// `alt`; `x` also wants `b2`, which only `alt` keeps, and `a0`, which
    /// nobody keeps and two agents offer.
    fn tracker() -> Tracker {
        let mut t = Tracker::new();
        t.add_agent(Agent::build("x").with_wants(vec![
            Behavior::build("b2"),
            Behavior::build("b1"),
            Behavior::build("a0"),
        ]));
        t.add_agent(Agent::build("y").with_wants(vec![Behavior::build("b1")]));
        t.add_agent(Agent::build("good").with_provides(vec![Behavior::build("b1")]));
        t.add_agent(
            Agent::build("alt").with_provides(vec![bc("b1", &["c", "d"]), bc("b2", &["c"])]),
        );
        t.add_agent(Agent::build("cprov").with_provides(vec![Behavior::build("c")]));
        t.add_agent(Agent::build("p1").with_provides(vec![bc("a0", &["c", "d"])]));
        t.add_agent(Agent::build("p2").with_provides(vec![bc("a0", &["d"])]));
        t
    }

    fn c() -> Resolution {
        Resolution::new("c").add_satisfying_offer(Offer::new("cprov"))
    }

    #[test]
    fn a_cause_names_the_variable_and_what_held_for_any_value() {
        let site = |agent: &str| {
            Some(query::CauseSite {
                agent: agent.into(),
                provide: "web is served".into(),
            })
        };
        let line = causes(&[
            Cause {
                behavior: "web runs on {{node}}".into(),
                kind: CauseKind::Unfound {
                    variables: vec!["node".into()],
                    holds_for_any: vec![HoldsForAny {
                        variable: "node".into(),
                        agent: "host".into(),
                        promise: "{{app}} runs on {{node}}".into(),
                    }],
                },
                needed_by: site("op"),
            },
            Cause {
                behavior: "{{node}} serves web".into(),
                kind: CauseKind::Unfound {
                    variables: vec!["node".into()],
                    holds_for_any: vec![],
                },
                needed_by: site("op"),
            },
        ]);
        assert_eq!(
            line,
            "web runs on {{node}} finds no value for `node`: only host's `{{app}} runs on {{node}}` answered, holding for any value (condition of web is served by op); \
             {{node}} serves web finds no value for `node` (condition of web is served by op)"
        );
    }

    #[test]
    fn resolutions_are_one_per_want_in_order() {
        let got = resolutions(&tracker(), false);
        assert_eq!(
            got,
            vec![
                Resolution::new("a0")
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "p1",
                        vec![c(), Resolution::new("d")]
                    ))
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "p2",
                        vec![Resolution::new("d")]
                    )),
                Resolution::new("b1")
                    .add_satisfying_offer(Offer::new("good"))
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "alt",
                        vec![c(), Resolution::new("d")]
                    )),
                Resolution::new("b2")
                    .add_satisfying_offer(Offer::new_conditional("alt", vec![c()])),
            ]
        );
    }

    #[test]
    fn resolutions_fold_unneeded_alternatives_when_asked() {
        let got = resolutions(&tracker(), true);
        assert_eq!(
            got,
            vec![
                // nobody keeps it: every offer stays
                Resolution::new("a0")
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "p1",
                        vec![c(), Resolution::new("d")]
                    ))
                    .add_unsatisfying_offer(Offer::new_conditional(
                        "p2",
                        vec![Resolution::new("d")]
                    )),
                Resolution::new("b1").add_satisfying_offer(Offer::new("good")),
                Resolution::new("b2")
                    .add_satisfying_offer(Offer::new_conditional("alt", vec![c()])),
            ]
        );
        let lines: Vec<String> = got
            .iter()
            .flat_map(|r| r.to_strings_compressed(false))
            .collect();
        assert_eq!(
            lines,
            vec![
                "a0 |-> p1 &-> c |-> cprov",
                "          &-> d |-> ?",
                "   |-> p2 &-> d |-> ?",
                "b1 |-> good",
                "b2 |-> alt &-> c |-> cprov",
            ]
        );
    }

    /// The repository's `samples/`: the agents the option leaves with no
    /// promise drawn are off the Overview graph, and on it with the option
    /// off.
    #[test]
    fn samples_lose_the_agents_only_an_unneeded_alternative_reached() {
        use promise_tracker::promise_graph::{promise_graph, promise_graph_with};

        let samples = format!("{}/../samples", env!("CARGO_MANIFEST_DIR"));
        let tracker = cli::load_tracker(&vec![samples], false);
        let plain = promise_graph(&tracker);
        let folded = promise_graph_with(&tracker, true);
        let ids = |g: &promise_tracker::promise_graph::PromiseGraphData| -> Vec<String> {
            g.nodes.iter().map(|n| n.id.clone()).collect()
        };
        let gone: Vec<String> = ids(&plain)
            .into_iter()
            .filter(|id| !ids(&folded).contains(id))
            .collect();
        assert_eq!(gone, vec!["aforb1-2", "ay1", "az1", "missing:y1", "missing:z2"]);
        assert_eq!(folded.unsatisfied, plain.unsatisfied);
    }
}
