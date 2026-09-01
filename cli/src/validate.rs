use clap::Parser;
use promise_tracker::validate::check_items;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// File(s) to validate
    #[clap(short, long = "file")]
    files: Vec<String>,
}

pub fn command(parameters: &Parameters) {
    let todo = match cli::ManifestList::new(&parameters.files) {
        Ok(todo) => todo,
        Err(e) => cli::abort(e),
    };
    let mut problems = 0;
    for file in todo.files {
        match cli::check_file(&file) {
            Ok(items) => {
                for item in items.iter() {
                    println!("Found: {}", item.get_name());
                }
                for problem in check_items(&items) {
                    problems += 1;
                    eprintln!("{}: {}", file, problem);
                }
            }
            Err(e) => cli::abort(e),
        }
    }
    if problems > 0 {
        process::exit(1);
    }
}
