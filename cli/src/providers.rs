use clap::Parser;
use promise_tracker::marks::Marks;
use promise_tracker::query::ConditionStatus;
use promise_tracker::query::{self, ConsumerKind, QueryOptions};
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show SuperAgent members as agents of their own
    #[clap(long)]
    unfold: bool,

    /// Leave out promises an agent makes only to itself
    #[clap(long)]
    fold_self: bool,

    /// Collapse rows that differ only by agent
    #[clap(long)]
    bundle: bool,

    /// Wrap values a variable was replaced with in `<>`, as each row's
    /// agents have them
    #[clap(long)]
    mark_bound: bool,

    /// Leave out providers whose promise is not kept, where another
    /// provider keeps the behavior; one nobody keeps still shows them all
    #[clap(long)]
    fold_alternatives: bool,

    /// The behavior to show providers and consumers for
    behavior: String,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    let options = QueryOptions {
        fold_self: parameters.fold_self,
        bundle: parameters.bundle,
        fold_alternatives: parameters.fold_alternatives,
    };
    cli::require_concrete(&parameters.behavior);
    let Some(found) = query::behavior_use(&tracker, &parameters.behavior, options) else {
        println!("Behavior {} not found", parameters.behavior);
        process::exit(1);
    };
    // Display only: the rows keep the plain names.
    let marks = parameters.mark_bound.then(|| Marks::new(&tracker));
    // A promise as the row's agents wrote that one promise.
    let promised = |agents: &[String], name: &String| -> String {
        match &marks {
            Some(marks) => marks.made_by(agents, name, name),
            None => name.clone(),
        }
    };
    println!("providers:");
    for row in &found.providers {
        let mut line = format!("  {}", cli::agent_list(&row.agents));
        if let Some(pattern) = &row.pattern {
            line.push_str(&format!(
                "\tvia {} (pattern)",
                promised(&row.agents, pattern)
            ));
        }
        if !row.conditions.is_empty() {
            let conditions: Vec<ConditionStatus> = row
                .conditions
                .iter()
                .map(|c| ConditionStatus {
                    behavior: match &marks {
                        // as the promise keeping this behavior has it
                        Some(marks) => {
                            marks.condition_by(&row.agents, &parameters.behavior, &c.behavior)
                        }
                        None => c.behavior.clone(),
                    },
                    held: c.held,
                })
                .collect();
            line.push_str(&format!(
                "\tif {}",
                crate::provides::conditions(&conditions)
            ));
        }
        println!("{}", line);
    }
    println!("consumers:");
    for row in &found.consumers {
        let how = match &row.kind {
            ConsumerKind::Want => "wants".to_string(),
            ConsumerKind::Condition { provide } => {
                format!("condition of {}", promised(&row.agents, provide))
            }
        };
        println!("  {}\t{}", cli::agent_list(&row.agents), how);
    }
}
