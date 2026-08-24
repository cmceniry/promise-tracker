use clap::Parser;
use promise_tracker::Tracker;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,
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
    let mut agent_names = tracker.get_agent_names();
    agent_names.sort();
    for agent_name in agent_names {
        println!("{}", agent_name);
    }
}
