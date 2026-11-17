use clap::Parser;
use promise_tracker::query::{self, QueryOptions};

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show SuperAgent members as agents of their own
    #[clap(long)]
    unfold: bool,

    /// Collapse providers that differ only by agent
    #[clap(long)]
    bundle: bool,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    // Self folding never changes this list: a self-promise is used by its
    // own provider, and folded away rather than unused.
    let options = QueryOptions {
        fold_self: false,
        bundle: parameters.bundle,
    };
    for group in query::unused(&tracker, options) {
        if group.pattern {
            println!("{}\t(pattern)", group.behavior);
        } else {
            println!("{}", group.behavior);
        }
        for provider in &group.providers {
            if provider.conditions.is_empty() {
                println!("  {}", cli::agent_list(&provider.agents));
            } else {
                println!(
                    "  {}\tif {}",
                    cli::agent_list(&provider.agents),
                    provider.conditions.join(", ")
                );
            }
        }
    }
}
