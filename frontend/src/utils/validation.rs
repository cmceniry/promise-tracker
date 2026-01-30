use crate::models::Contract;
use promise_tracker::components::Item;
use serde::Deserialize;

/// Validates a contract filename according to the rules:
/// - Must only contain lowercase a-z, digits 0-9, forward slash (/)
/// - Must end with ".yaml"
/// - Cannot start with "/"
/// - Cannot be empty
///
/// Returns Some(error_message) if invalid, None if valid
pub fn validate_filename(filename: &str) -> Option<String> {
    let trimmed = filename.trim();

    if trimmed.is_empty() {
        return Some("Filename cannot be empty".to_string());
    }

    // Cannot start with "/"
    if trimmed.starts_with('/') {
        return Some("Filename cannot start with \"/\"".to_string());
    }

    // Must end with ".yaml"
    if !trimmed.ends_with(".yaml") {
        return Some("Filename must end with \".yaml\"".to_string());
    }

    // Get the name part (everything before ".yaml")
    let name_part = &trimmed[..trimmed.len() - 5];

    if name_part.is_empty() {
        return Some("Filename must have a name before \".yaml\"".to_string());
    }

    // Check each character in the name part
    for ch in name_part.chars() {
        let is_lowercase = ch.is_ascii_lowercase();
        let is_digit = ch.is_ascii_digit();
        let is_slash = ch == '/';

        if !is_lowercase && !is_digit && !is_slash {
            return Some(format!(
                "Filename contains invalid character \"{}\". Only lowercase letters (a-z), digits (0-9), and forward slash (/) are allowed.",
                ch
            ));
        }
    }

    None // Valid
}

/// Generates a random 8-character lowercase alphabetic filename with ".yaml" suffix
pub fn generate_random_filename() -> String {
    use js_sys::Math;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    let mut result = String::with_capacity(13); // 8 chars + ".yaml"
    for _ in 0..8 {
        let idx = (Math::random() * CHARS.len() as f64) as usize;
        result.push(CHARS[idx] as char);
    }
    result.push_str(".yaml");
    result
}

/// Generates a unique random filename by checking against existing contracts
pub fn generate_unique_random_filename(
    existing_contracts: &[Contract],
    max_attempts: usize,
) -> String {
    use js_sys::Date;
    use std::collections::HashSet;

    let existing_filenames: HashSet<&str> = existing_contracts
        .iter()
        .map(|c| c.filename.as_str())
        .filter(|f| !f.trim().is_empty())
        .collect();

    for _ in 0..max_attempts {
        let filename = generate_random_filename();
        if !existing_filenames.contains(filename.as_str()) {
            return filename;
        }
    }

    // If we've exhausted attempts, append a timestamp to make it unique
    let base_name = &generate_random_filename()[..8]; // Remove .yaml
    let timestamp = Date::now() as u64;
    // Convert to base36-like string (using simple hex for simplicity)
    let timestamp_suffix = format!("{:x}", timestamp % 0xFFFF);
    format!(
        "{}{}.yaml",
        base_name,
        &timestamp_suffix[timestamp_suffix.len().saturating_sub(4)..]
    )
}

/// Validates a contract's YAML content against the contract schema
/// Returns an error message if invalid, empty string if valid
pub fn validate_contract_content(content: &str) -> String {
    if content.trim().is_empty() {
        return String::new(); // Empty content is valid (new contract)
    }

    let mut item_count = 0;

    for (idx, document) in serde_yaml::Deserializer::from_str(content).enumerate() {
        match Item::deserialize(document) {
            Ok(_item) => {
                item_count += 1;
            }
            Err(e) => {
                return format!("Document {}: {}", idx + 1, e);
            }
        }
    }

    if item_count == 0 {
        return "Contract must contain at least one Agent or SuperAgent".to_string();
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_filename_valid() {
        assert!(validate_filename("test.yaml").is_none());
        assert!(validate_filename("mycontract.yaml").is_none());
        assert!(validate_filename("path/to/file.yaml").is_none());
        assert!(validate_filename("abc123.yaml").is_none());
    }

    #[test]
    fn test_validate_filename_invalid() {
        assert!(validate_filename("").is_some());
        assert!(validate_filename("   ").is_some());
        assert!(validate_filename("/test.yaml").is_some());
        assert!(validate_filename("test.yml").is_some());
        assert!(validate_filename("Test.yaml").is_some()); // uppercase
        assert!(validate_filename("test-file.yaml").is_some()); // hyphen
        assert!(validate_filename(".yaml").is_some()); // no name
    }

    #[test]
    fn test_validate_contract_content_valid_agent() {
        let content = "kind: Agent\nname: test";
        assert!(validate_contract_content(content).is_empty());
    }

    #[test]
    fn test_validate_contract_content_invalid_kind() {
        let content = "kind: InvalidType\nname: test";
        assert!(!validate_contract_content(content).is_empty());
    }

    #[test]
    fn test_validate_contract_content_unknown_field() {
        let content = "kind: Agent\nname: test\nunknownField: value";
        assert!(!validate_contract_content(content).is_empty());
    }

    #[test]
    fn test_validate_contract_content_missing_name() {
        let content = "kind: Agent";
        assert!(!validate_contract_content(content).is_empty());
    }
}
