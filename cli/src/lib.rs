use promise_tracker::components::Item;
use promise_tracker::Tracker;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug)]
pub enum AddError {
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

pub struct ManifestList {
    pub files: HashSet<String>,
}

impl ManifestList {
    pub fn new(files: &Vec<String>) -> Result<ManifestList, AddError> {
        let mut m = ManifestList {
            files: HashSet::new(),
        };
        for path in files {
            match m.add(&path) {
                Ok(_) => {}
                Err(e) => {
                    return Err(AddError::Io(e));
                }
            };
        }
        Ok(m)
    }

    pub fn add(&mut self, path: &str) -> Result<(), std::io::Error> {
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

pub fn check_file(path: &str) -> Result<Vec<Item>, AddError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) => return Err(AddError::Io(e)),
    };
    let mut ret: Vec<Item> = vec![];
    for document in serde_yaml::Deserializer::from_str(&contents) {
        match Item::deserialize(document) {
            Ok(item) => ret.push(item),
            Err(e) => return Err(AddError::Yaml(e)),
        }
    }
    Ok(ret)
}

pub fn process_file(path: &str, tracker: &mut Tracker) -> Result<(), AddError> {
    let items = check_file(path)?;
    for item in items {
        tracker.add_item(item);
    }
    Ok(())
}
