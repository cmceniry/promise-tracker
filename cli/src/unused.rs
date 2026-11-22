use clap::Parser;
use promise_tracker::marks::Marks;
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

    /// Wrap values a variable was replaced with in `<>`, as the agents
    /// making each promise wrote it
    #[clap(long)]
    mark_bound: bool,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    // Self folding never changes this list: a self-promise is used by its
    // own provider, and folded away rather than unused.
    let options = QueryOptions {
        fold_self: false,
        bundle: parameters.bundle,
    };
    // Display only: the rows keep the plain names.
    let marks = parameters.mark_bound.then(|| Marks::new(&tracker));
    for group in query::unused(&tracker, options) {
        let behavior = match &marks {
            Some(marks) => {
                let makers: Vec<&String> = group.providers.iter().flat_map(|p| &p.agents).collect();
                marks.made_by(&makers, &group.behavior, &group.behavior)
            }
            None => group.behavior.clone(),
        };
        if group.pattern {
            println!("{}\t(pattern)", behavior);
        } else {
            println!("{}", behavior);
        }
        for provider in &group.providers {
            if provider.conditions.is_empty() {
                println!("  {}", cli::agent_list(&provider.agents));
            } else {
                println!(
                    "  {}\tif {}",
                    cli::agent_list(&provider.agents),
                    provider
                        .conditions
                        .iter()
                        .map(|c| match &marks {
                            Some(marks) => {
                                marks.condition_by(&provider.agents, &group.behavior, c)
                            }
                            None => c.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }
}
