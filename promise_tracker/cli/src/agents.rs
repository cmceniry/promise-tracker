use clap::Parser;
use promise_tracker::Tracker;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,
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
    let mut agent_names = tracker.get_agent_names();
    agent_names.sort();
    for agent_name in agent_names {
        println!("{}", agent_name);
    }
}
