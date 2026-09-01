use clap::Parser;
use promise_tracker::validate::check_items;
use promise_tracker::Tracker;
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
    let mut tracker = Tracker::new();
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
                for item in items {
                    tracker.add_item(item);
                }
            }
            Err(e) => cli::abort(e),
        }
    }

    // These need every file in play, so they wait until all of them are in: a
    // base may perfectly well live in a different document than its instance.
    for cycle in tracker.membership_cycles() {
        problems += 1;
        let mut ring: Vec<&str> = cycle.iter().map(|n| n.as_str()).collect();
        if let Some(first) = cycle.first() {
            ring.push(first);
        }
        eprintln!(
            "SuperAgent/{}: collectives contain each other ({}); each makes a template of the next, so none of them stands as an agent",
            cycle[0],
            ring.join(" -> ")
        );
    }
    for (instance, base) in tracker.dangling_instance_bases() {
        problems += 1;
        eprintln!(
            "Instance/{}: base `{}` names nothing loaded here",
            instance, base
        );
    }
    for (agent, want) in tracker.non_ground_wants() {
        problems += 1;
        eprintln!(
            "{} wants `{}`, which names no one behavior; bind it from an Instance or write it out",
            agent, want
        );
    }

    if problems > 0 {
        process::exit(1);
    }
}
