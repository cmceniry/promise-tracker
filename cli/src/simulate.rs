use clap::Parser;
use promise_tracker::Tracker;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// The behavior to validate
    behavior: String,
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
    println!("{:?}", tracker.resolve(&parameters.behavior));
}
