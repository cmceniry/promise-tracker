use clap::Parser;
use promise_tracker::Tracker;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// The agent name to show actions for
    agent: String,
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
    let mut provides: Vec<String> = vec![];
    if let Some(pa) = tracker.get_agent_provides(&parameters.agent) {
        for p in pa {
            provides.push(p);
        }
    } else {
        println!("Agent {} not found", parameters.agent);
        process::exit(1);
    }
    provides.sort();
    for p in provides {
        println!("{}", p);
    }
}
