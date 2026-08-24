use clap::Parser;
use promise_tracker::Tracker;
use std::collections::HashSet;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show single line outputs per provides/conditions
    #[clap(short, long)]
    compressed: bool,
}

pub fn command(parameters: &Parameters) {
    let mut tracker = Tracker::new();
    let todo = match cli::ManifestList::new(&parameters.files) {
        Ok(todo) => todo,
        Err(e) => cli::abort(e),
    };
    for file in todo.files {
        match cli::process_file(&file, &mut tracker) {
            Ok(_) => {}
            Err(e) => cli::abort(e),
        }
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
