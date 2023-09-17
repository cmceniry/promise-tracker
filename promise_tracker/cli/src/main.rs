use clap::{Parser, Subcommand};

mod schema;
mod validate;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Display the json_schema for Agents et al
    Schema {},
    /// Check that the given file(s) are yaml and contain valid Agents et al
    Validate(validate::Parameters),
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Schema {}) => {
            schema::command();
        }
        Some(Command::Validate(parameters)) => {
            validate::command(parameters);
        }
        None => {}
    }
}
