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
    let mut behaviors: Vec<String> = tracker.get_working_behaviors().into_iter().collect();
    behaviors.sort();
    for behavior_name in behaviors {
        println!("{}", behavior_name);
    }
    // Patterns are not behaviors — each stands for a family of them — so they
    // are listed apart and marked.
    let mut patterns: Vec<String> = tracker.get_behavior_patterns().into_iter().collect();
    patterns.sort();
    for pattern in patterns {
        println!("{}\t(pattern)", pattern);
    }
}
