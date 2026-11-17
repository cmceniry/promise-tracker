use clap::Parser;
use promise_tracker::query::{self, ConditionStatus, QueryOptions};
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show SuperAgent members as agents of their own
    #[clap(long)]
    unfold: bool,

    /// Leave out promises the agent makes only to itself
    #[clap(long)]
    fold_self: bool,

    /// Show each conditional provide's conditions, held or broken
    #[clap(short, long)]
    conditions: bool,

    /// The agent name to show actions for
    agent: String,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    let options = QueryOptions {
        fold_self: parameters.fold_self,
        bundle: false,
    };
    let Some(provides) = query::agent_provides(&tracker, &parameters.agent, options) else {
        println!("Agent {} not found", parameters.agent);
        process::exit(1);
    };
    if !parameters.conditions {
        // One line per behavior and per pattern, as the CLI always printed.
        let mut behaviors: Vec<&String> = provides.provides.iter().map(|r| &r.behavior).collect();
        behaviors.dedup();
        for behavior in behaviors {
            println!("{}", behavior);
        }
        let mut patterns: Vec<&String> = provides.patterns.iter().map(|r| &r.pattern).collect();
        patterns.dedup();
        for pattern in patterns {
            println!("{}\t(pattern)", pattern);
        }
        return;
    }
    for row in provides.provides {
        if row.conditions.is_empty() {
            println!("{}", row.behavior);
        } else {
            println!("{}\tif {}", row.behavior, conditions(&row.conditions));
        }
    }
    for row in provides.patterns {
        if row.conditions.is_empty() {
            println!("{}\t(pattern)", row.pattern);
        } else {
            println!(
                "{}\t(pattern) if {}",
                row.pattern,
                row.conditions.join(", ")
            );
        }
    }
}

/// `c1 (held), c2 (broken)`
pub fn conditions(conditions: &[ConditionStatus]) -> String {
    conditions
        .iter()
        .map(|c| {
            format!(
                "{} ({})",
                c.behavior,
                if c.held { "held" } else { "broken" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
