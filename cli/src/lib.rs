use promise_tracker::components::Item;
use promise_tracker::Tracker;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug)]
pub enum AddError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Yaml {
        path: String,
        source: serde_yaml::Error,
        /// The offending source line, when the error carries a location.
        snippet: Option<String>,
    },
}

impl AddError {
    fn io(path: &str, source: std::io::Error) -> AddError {
        AddError::Io {
            path: path.to_string(),
            source: source,
        }
    }

    fn yaml(path: &str, source: serde_yaml::Error, contents: &str) -> AddError {
        let snippet = source
            .location()
            .and_then(|l| render_snippet(contents, l.line(), l.column()));
        AddError::Yaml {
            path: path.to_string(),
            source: source,
            snippet: snippet,
        }
    }

    /// Where the problem is, as `line:column`, when known.
    pub fn location(&self) -> Option<(usize, usize)> {
        match self {
            AddError::Io { .. } => None,
            AddError::Yaml { source, .. } => source.location().map(|l| (l.line(), l.column())),
        }
    }
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AddError::Io { path, source } => write!(f, "{}: {}", path, source),
            AddError::Yaml {
                path,
                source,
                snippet,
            } => {
                let message = source.to_string();
                match source.location() {
                    Some(l) => {
                        write!(
                            f,
                            "{}:{}:{}: {}",
                            path,
                            l.line(),
                            l.column(),
                            strip_location(&message)
                        )?;
                    }
                    None => write!(f, "{}: {}", path, message)?,
                }
                match snippet {
                    Some(s) => write!(f, "\n{}", s),
                    None => Ok(()),
                }
            }
        }
    }
}

impl std::error::Error for AddError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AddError::Io { source, .. } => Some(source),
            AddError::Yaml { source, .. } => Some(source),
        }
    }
}

/// serde_yaml writes " at line N column M" into its messages, at the end for
/// deserialization errors and mid sentence for scanner errors. The location is
/// reported separately, next to the file name, so drop the duplicate.
fn strip_location(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find(" at line ") {
        match position_len(&rest[start..]) {
            Some(len) => {
                out.push_str(&rest[..start]);
                rest = &rest[start + len..];
            }
            None => {
                let keep = start + " at line ".len();
                out.push_str(&rest[..keep]);
                rest = &rest[keep..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The length of a leading " at line N column M", if that is what starts here.
fn position_len(s: &str) -> Option<usize> {
    let mut len = " at line ".len();
    let line = s[len..].chars().take_while(|c| c.is_ascii_digit()).count();
    if line == 0 {
        return None;
    }
    len += line;
    if !s[len..].starts_with(" column ") {
        return None;
    }
    len += " column ".len();
    let column = s[len..].chars().take_while(|c| c.is_ascii_digit()).count();
    if column == 0 {
        return None;
    }
    Some(len + column)
}

/// A rustc style pointer at the offending line:
///
/// ```text
///   |
/// 5 |     bogus: 1
///   |     ^
/// ```
fn render_snippet(contents: &str, line: usize, column: usize) -> Option<String> {
    let text = contents.lines().nth(line.checked_sub(1)?)?;
    let number = line.to_string();
    let gutter = " ".repeat(number.len());
    // Columns are 1 based and counted in characters, and tabs are widened to
    // a single space so the caret keeps lining up with what is printed.
    let printable: String = text
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect();
    let pad = " ".repeat(column.saturating_sub(1).min(printable.chars().count()));
    Some(format!(
        "{} |\n{} | {}\n{} | {}^",
        gutter, number, printable, gutter, pad
    ))
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
                    return Err(AddError::io(path, e));
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
        Err(e) => return Err(AddError::io(path, e)),
    };
    let mut ret: Vec<Item> = vec![];
    for document in serde_yaml::Deserializer::from_str(&contents) {
        match Item::deserialize(document) {
            Ok(item) => ret.push(item),
            Err(e) => return Err(AddError::yaml(path, e, &contents)),
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

/// Report a load failure and stop, for the commands that cannot continue
/// without their contracts.
pub fn abort(e: AddError) -> ! {
    eprintln!("error: {}", e);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(contents: &str) -> String {
        let e = serde_yaml::Deserializer::from_str(contents)
            .map(Item::deserialize)
            .collect::<Result<Vec<Item>, serde_yaml::Error>>()
            .expect_err("expected a parse failure");
        AddError::yaml("contracts.yaml", e, contents).to_string()
    }

    #[test]
    fn reports_file_line_and_column() {
        let out = report("kind: Agent\nname: a\nprovides:\n  - name: http\n    bogus: 1\n");
        assert_eq!(
            out.lines().next().unwrap(),
            "contracts.yaml:5:5: provides[0]: unknown field `bogus`, \
             expected one of `name`, `comment`, `conditions`"
        );
    }

    #[test]
    fn points_at_the_offending_column() {
        let out = report("kind: Agent\nname: a\nprovides:\n  - name: http\n    bogus: 1\n");
        assert_eq!(
            out.lines().skip(1).collect::<Vec<&str>>(),
            vec!["  |", "5 |     bogus: 1", "  |     ^"]
        );
    }

    #[test]
    fn keeps_a_single_copy_of_the_location() {
        let out = report("kind: Agent\nname:\n  - a\n");
        assert_eq!(out.matches("line").count(), 0);
        assert!(out.starts_with("contracts.yaml:3:3: "), "{}", out);
    }

    #[test]
    fn drops_the_location_from_scanner_messages() {
        assert_eq!(
            strip_location("did not find expected ',' at line 4 column 1, while parsing"),
            "did not find expected ',', while parsing"
        );
        assert_eq!(
            strip_location("invalid type: sequence at line 6 column 3"),
            "invalid type: sequence"
        );
    }

    #[test]
    fn keeps_prose_that_only_looks_like_a_location() {
        let message = "unknown field `at line width`, expected one of `name`";
        assert_eq!(strip_location(message), message);
    }

    #[test]
    fn snippet_is_skipped_when_the_line_is_past_the_end() {
        assert_eq!(render_snippet("kind: Agent\n", 4, 1), None);
    }

    #[test]
    fn snippet_caret_stops_at_the_end_of_the_line() {
        let snippet = render_snippet("kind: Agent\n", 1, 40).unwrap();
        assert_eq!(
            snippet.lines().last().unwrap(),
            format!("  | {}^", " ".repeat("kind: Agent".len()))
        );
    }

    #[test]
    fn missing_files_name_the_path() {
        let e = check_file("no/such/contract.yaml").expect_err("expected a read failure");
        assert!(e.to_string().starts_with("no/such/contract.yaml: "), "{}", e);
        assert_eq!(e.location(), None);
    }
}
