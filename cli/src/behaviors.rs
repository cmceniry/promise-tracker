use clap::Parser;
use promise_tracker::Tracker;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
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
    let mut behaviors: Vec<String> = vec![];
    for behavior_name in tracker.get_working_behaviors() {
        behaviors.push(behavior_name);
    }
    behaviors.sort();
    for behavior_name in behaviors {
        println!("{}", behavior_name);
    }
}
