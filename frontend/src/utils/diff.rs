use similar::{ChangeTag, TextDiff};

/// Represents a single line in the diff output
#[derive(Clone, Debug, PartialEq)]
pub enum DiffLineType {
    /// Line exists in both versions (unchanged)
    Unchanged,
    /// Line was added in the new version
    Added,
    /// Line was removed from the old version
    Removed,
    /// Empty placeholder for alignment in side-by-side view
    Empty,
}

/// A line in the diff view
#[derive(Clone, Debug)]
pub struct DiffLine {
    pub text: String,
    pub line_type: DiffLineType,
}

/// Result of computing a side-by-side diff
#[derive(Clone, Debug)]
pub struct SideBySideDiff {
    /// Left column (new/local version)
    pub left: Vec<DiffLine>,
    /// Right column (old/server version)
    pub right: Vec<DiffLine>,
}

/// Normalize content for comparison
/// - Trims whitespace from start and end
/// - Normalizes line endings to \n
/// - Removes trailing whitespace from lines
pub fn normalize_content(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    text.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Compare two contract texts (normalized)
/// Returns true if contracts are different, false if same
pub fn compare_contracts(local_text: &str, server_text: &str) -> bool {
    let normalized_local = normalize_content(local_text);
    let normalized_server = normalize_content(server_text);
    normalized_local != normalized_server
}

/// Check if local filename differs from server path
/// Returns true if filename differs from server path, false if same or no server path
pub fn check_filename_diff(local_filename: &str, server_path: Option<&str>) -> bool {
    match server_path {
        Some(path) => {
            let normalized_local = local_filename.trim();
            let normalized_server = path.trim();
            normalized_local != normalized_server
        }
        None => false, // No server path to compare against
    }
}

/// Compute a side-by-side diff between new (local) and old (server) text
pub fn compute_side_by_side_diff(new_text: &str, old_text: Option<&str>) -> SideBySideDiff {
    // Handle null old_text (contract doesn't exist on server)
    let old_normalized = match old_text {
        Some(text) => {
            let mut s = text.trim().to_string();
            if !s.is_empty() {
                s.push('\n');
            }
            s
        }
        None => String::new(),
    };

    let new_normalized = {
        let mut s = new_text.trim().to_string();
        if !s.is_empty() {
            s.push('\n');
        }
        s
    };

    let diff = TextDiff::from_lines(&old_normalized, &new_normalized);

    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();

    // Collect changes into groups
    let mut pending_removes: Vec<&str> = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                // First flush any pending removes
                for removed_line in pending_removes.drain(..) {
                    left_lines.push(DiffLine {
                        text: String::new(),
                        line_type: DiffLineType::Empty,
                    });
                    right_lines.push(DiffLine {
                        text: removed_line.trim_end_matches('\n').to_string(),
                        line_type: DiffLineType::Removed,
                    });
                }

                // Add unchanged line to both sides
                let line_text = change.value().trim_end_matches('\n').to_string();
                left_lines.push(DiffLine {
                    text: line_text.clone(),
                    line_type: DiffLineType::Unchanged,
                });
                right_lines.push(DiffLine {
                    text: line_text,
                    line_type: DiffLineType::Unchanged,
                });
            }
            ChangeTag::Delete => {
                // Line was in old but not in new - collect for potential pairing
                pending_removes.push(change.value());
            }
            ChangeTag::Insert => {
                // Line was added in new
                let line_text = change.value().trim_end_matches('\n').to_string();

                if let Some(removed_line) = pending_removes.pop() {
                    // Pair with a removed line for side-by-side view
                    left_lines.push(DiffLine {
                        text: line_text,
                        line_type: DiffLineType::Added,
                    });
                    right_lines.push(DiffLine {
                        text: removed_line.trim_end_matches('\n').to_string(),
                        line_type: DiffLineType::Removed,
                    });
                } else {
                    // No removed line to pair with
                    left_lines.push(DiffLine {
                        text: line_text,
                        line_type: DiffLineType::Added,
                    });
                    right_lines.push(DiffLine {
                        text: String::new(),
                        line_type: DiffLineType::Empty,
                    });
                }
            }
        }
    }

    // Flush any remaining pending removes
    for removed_line in pending_removes.drain(..) {
        left_lines.push(DiffLine {
            text: String::new(),
            line_type: DiffLineType::Empty,
        });
        right_lines.push(DiffLine {
            text: removed_line.trim_end_matches('\n').to_string(),
            line_type: DiffLineType::Removed,
        });
    }

    SideBySideDiff {
        left: left_lines,
        right: right_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_content() {
        assert_eq!(normalize_content("  hello  "), "hello");
        assert_eq!(normalize_content("line1\nline2"), "line1\nline2");
        assert_eq!(normalize_content("line1  \nline2  "), "line1\nline2");
        assert_eq!(normalize_content(""), "");
    }

    #[test]
    fn test_compare_contracts() {
        assert!(!compare_contracts("hello", "hello"));
        assert!(!compare_contracts("hello  ", "  hello"));
        assert!(compare_contracts("hello", "world"));
        assert!(compare_contracts("line1\nline2", "line1"));
    }

    #[test]
    fn test_check_filename_diff() {
        assert!(!check_filename_diff("test.yaml", None));
        assert!(!check_filename_diff("test.yaml", Some("test.yaml")));
        assert!(check_filename_diff("test.yaml", Some("other.yaml")));
        assert!(!check_filename_diff("  test.yaml  ", Some("test.yaml")));
    }

    #[test]
    fn test_compute_side_by_side_diff_no_old() {
        let diff = compute_side_by_side_diff("line1\nline2", None);
        assert_eq!(diff.left.len(), 2);
        assert_eq!(diff.right.len(), 2);
        assert_eq!(diff.left[0].line_type, DiffLineType::Added);
        assert_eq!(diff.right[0].line_type, DiffLineType::Empty);
    }

    #[test]
    fn test_compute_side_by_side_diff_unchanged() {
        let diff = compute_side_by_side_diff("line1\nline2", Some("line1\nline2"));
        assert_eq!(diff.left.len(), 2);
        assert_eq!(diff.right.len(), 2);
        assert_eq!(diff.left[0].line_type, DiffLineType::Unchanged);
        assert_eq!(diff.right[0].line_type, DiffLineType::Unchanged);
    }

    #[test]
    fn test_compute_side_by_side_diff_mixed() {
        let diff =
            compute_side_by_side_diff("line1\nmodified\nline3", Some("line1\noriginal\nline3"));
        assert_eq!(diff.left.len(), 3);
        assert_eq!(diff.right.len(), 3);
        // line1 unchanged
        assert_eq!(diff.left[0].line_type, DiffLineType::Unchanged);
        // middle line changed
        assert_eq!(diff.left[1].line_type, DiffLineType::Added);
        assert_eq!(diff.right[1].line_type, DiffLineType::Removed);
        // line3 unchanged
        assert_eq!(diff.left[2].line_type, DiffLineType::Unchanged);
    }
}
