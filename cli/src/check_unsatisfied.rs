use clap::Parser;
use promise_tracker::query::{self, Cause, CauseKind, QueryOptions};
use std::collections::HashSet;

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
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    if parameters.summary || parameters.bundle {
        let options = QueryOptions {
            fold_self: false,
            bundle: parameters.bundle,
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
    let mut wants = HashSet::new();
    for agent_name in tracker.get_working_agent_names() {
        for want in tracker.get_agent_wants(agent_name.clone()) {
            wants.insert(want);
        }
    }
    let mut wants_ordered: Vec<_> = wants.iter().collect();
    wants_ordered.sort();
    for want in wants_ordered {
        let r = tracker.resolve(&want);
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
                cause.kind,
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
                CauseKind::Unprovided if behavior.contains("{{?") => {
                    "matches nothing promised outright (parameterized promises are not considered)"
                }
                CauseKind::Unprovided => "is not provided",
                CauseKind::Cycle => "is only provided through a dependency loop",
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
