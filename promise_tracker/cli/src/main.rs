use clap::{Parser, Subcommand};

mod schema;
mod simulate;
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
    /// Check which behaviors are covered by the given file(s)
    Simulate(simulate::Parameters),
    /// Check that the given file(s) are yaml and contain valid Agents et al
    Validate(validate::Parameters),
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Schema {}) => {
            schema::command();
        }
        Some(Command::Simulate(parameters)) => {
            simulate::command(parameters);
        }
        Some(Command::Validate(parameters)) => {
            validate::command(parameters);
        }
        None => {}
    }
}
