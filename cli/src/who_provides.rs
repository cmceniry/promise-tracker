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
    let todo = cli::ManifestList::new(&parameters.files).unwrap();
    for file in todo.files {
        match cli::process_file(&file, &mut tracker) {
            Ok(_) => {}
            Err(e) => {
                println!("Error processing {}: {}", file, e);
                process::exit(1);
            }
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
