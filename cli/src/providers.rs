use clap::Parser;
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

    /// The behavior to show providers and consumers for
    behavior: String,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    let options = QueryOptions {
        fold_self: parameters.fold_self,
        bundle: parameters.bundle,
    };
    let Some(found) = query::behavior_use(&tracker, &parameters.behavior, options) else {
        println!("Behavior {} not found", parameters.behavior);
        process::exit(1);
    };
    println!("providers:");
    for row in &found.providers {
        let mut line = format!("  {}", cli::agent_list(&row.agents));
        if let Some(pattern) = &row.pattern {
            line.push_str(&format!("\tvia {} (pattern)", pattern));
        }
        if !row.conditions.is_empty() {
            line.push_str(&format!(
                "\tif {}",
                crate::provides::conditions(&row.conditions)
            ));
        }
        println!("{}", line);
    }
    println!("consumers:");
    for row in &found.consumers {
        let how = match &row.kind {
            ConsumerKind::Want => "wants".to_string(),
            ConsumerKind::Condition { provide } => format!("condition of {}", provide),
        };
        println!("  {}\t{}", cli::agent_list(&row.agents), how);
    }
}
