use clap::Parser;

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
    for file in todo.files {
        match cli::check_file(&file) {
            Ok(items) => {
                for item in items {
                    println!("Found: {}", item.get_name());
                }
            }
            Err(e) => cli::abort(e),
        }
    }
}
