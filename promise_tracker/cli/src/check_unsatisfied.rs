use clap::Parser;
use promise_tracker::Tracker;
use serde::Deserialize;
use std::collections::HashSet;
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    files: Vec<String>,
}

enum AddError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}
impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AddError::Io(e) => e.fmt(f),
            AddError::Yaml(e) => e.fmt(f),
        }
    }
}

struct ManifestList {
    files: HashSet<String>,
}

impl ManifestList {
    fn add(&mut self, path: &str) -> Result<(), std::io::Error> {
        if self.files.contains(path) {
            return Ok(());
        };
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => return Err(e),
        };
        if metadata.is_file() {
            self.files.insert(path.to_string());
            return Ok(());
        }
        if !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Path is not a file or directory",
            ));
        }
        let dir = match std::fs::read_dir(path) {
            Ok(d) => d,
            Err(e) => return Err(e),
        };
        for entry in dir {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => return Err(e),
            };
            match entry.path().to_str() {
                Some(p) => self.add(p)?,
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Path is not valid unicode",
                    ))
                }
            };
        }
        Ok(())
    }
}

fn process_file(path: &str, tracker: &mut Tracker) -> Result<(), AddError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) => return Err(AddError::Io(e)),
    };
    for document in serde_yaml::Deserializer::from_str(&contents) {
        match promise_tracker::components::Item::deserialize(document) {
            Ok(item) => tracker.add_item(item),
            Err(e) => return Err(AddError::Yaml(e)),
        }
    }
    Ok(())
}

pub fn command(parameters: &Parameters) {
    let mut tracker = Tracker::new();
    let mut todo = ManifestList {
        files: HashSet::new(),
    };
    parameters
        .files
        .iter()
        .for_each(|path| match todo.add(path) {
            Ok(_) => {}
            Err(e) => {
                println!("Error adding {}: {}", path, e);
                process::exit(1);
            }
        });
    for file in todo.files {
        match process_file(&file, &mut tracker) {
            Ok(_) => {}
            Err(e) => {
                println!("Error processing {}: {}", file, e);
                process::exit(1);
            }
        }
    }
    let mut wants = HashSet::new();
    for agent_name in tracker.get_working_agent_names() {
        for want in tracker.get_agent_wants(agent_name.clone()) {
            wants.insert(want);
        }
    }
    for want in wants {
        println!("{:?}", tracker.resolve(&want));
    }
}
