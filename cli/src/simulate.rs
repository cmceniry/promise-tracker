use clap::Parser;
use promise_tracker::Tracker;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Leave out offers that are not kept, and everything they rest on,
    /// where another offer already keeps the same behavior; a behavior
    /// nobody keeps still shows every offer
    #[clap(long)]
    fold_alternatives: bool,

    /// The behavior to validate
    behavior: String,
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
    cli::require_concrete(&parameters.behavior);
    let mut resolution = tracker.resolve(&parameters.behavior);
    if parameters.fold_alternatives {
        resolution = resolution.without_unneeded_alternatives();
    }
    println!("{:?}", resolution);
}
