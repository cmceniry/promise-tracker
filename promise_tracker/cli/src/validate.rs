use clap::Parser;
use serde::Deserialize;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// File(s) to validate
    files: Vec<String>,
}

pub fn command(parameters: &Parameters) {
    parameters.files.iter().for_each(|file| {
        let contents = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(e) => {
                println!("Error: {}", e);
                process::exit(1);
            },
        };
        for document in serde_yaml::Deserializer::from_str(&contents) {
            match promise_tracker::components::Item::deserialize(document) {
                Ok(item) => {
                    println!("Found: {}", item.get_name());
                }
                Err(e) => {
                    println!("Error: {}", e);
                    process::exit(2);
                }
            }
        }
    });
}