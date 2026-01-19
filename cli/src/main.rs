use clap::{Parser, Subcommand};

mod agents;
mod behaviors;
mod check_unsatisfied;
mod schema;
mod simulate;
mod validate;
mod who_provides;

#[derive(Parser)]
#[command(arg_required_else_help(true))]
struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List agents
    Agents(agents::Parameters),
    /// List behaviors (after resolving SuperAgents)
    Behavior(behaviors::Parameters),
    /// See what wants aren't satisfied
    CheckUnsatisfied(check_unsatisfied::Parameters),
    /// Display the json_schema for Agents et al
    Schema {},
    /// Check which behaviors are covered by the given file(s)
    Simulate(simulate::Parameters),
    /// Check that the given file(s) are yaml and contain valid Agents et al
    Validate(validate::Parameters),
    /// Show who provides stuff
    WhoProvides(who_provides::Parameters),
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Agents(parameters)) => {
            agents::command(parameters);
        }
        Some(Command::Behavior(parameters)) => {
            behaviors::command(parameters);
        }
        Some(Command::CheckUnsatisfied(parameters)) => {
            check_unsatisfied::command(parameters);
        }
        Some(Command::Schema {}) => {
            schema::command();
        }
        Some(Command::Simulate(parameters)) => {
            simulate::command(parameters);
        }
        Some(Command::Validate(parameters)) => {
            validate::command(parameters);
        }
        Some(Command::WhoProvides(parameters)) => {
            who_provides::command(parameters);
        }
        None => {}
    }
}
