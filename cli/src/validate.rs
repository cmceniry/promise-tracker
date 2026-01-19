use clap::Parser;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// File(s) to validate
    #[clap(short, long = "file")]
    files: Vec<String>,
}

pub fn command(parameters: &Parameters) {
    let todo = cli::ManifestList::new(&parameters.files).unwrap();
    for file in todo.files {
        match cli::check_file(&file) {
            Ok(items) => {
                for item in items {
                    println!("Found: {}", item.get_name());
                }
            }
            Err(e) => {
                println!("Error in {}: {}", file, e);
                process::exit(1);
            }
        }
    }
}
