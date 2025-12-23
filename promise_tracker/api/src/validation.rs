use promise_tracker::components::Item;
use serde::Deserialize;
use anyhow::Result;

/// Validation error type
#[derive(Debug)]
pub enum ValidationError {
    Yaml(serde_yaml::Error),
    InvalidContent(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ValidationError::Yaml(e) => write!(f, "YAML parsing error: {}", e),
            ValidationError::InvalidContent(msg) => write!(f, "Invalid content: {}", msg),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate a contract (multidoc YAML) containing only Agents and SuperAgents
/// Returns the parsed Items if valid
pub fn validate_contract(content: &str) -> Result<Vec<Item>, ValidationError> {
    let mut items: Vec<Item> = vec![];
    
    // Parse multidoc YAML
    for document in serde_yaml::Deserializer::from_str(content) {
        match Item::deserialize(document) {
            Ok(item) => {
                // Ensure it's either Agent or SuperAgent
                match &item {
                    Item::Agent(_) | Item::SuperAgent(_) => {
                        items.push(item);
                    }
                }
            }
            Err(e) => {
                return Err(ValidationError::Yaml(e));
            }
        }
    }

    if items.is_empty() {
        return Err(ValidationError::InvalidContent(
            "Contract must contain at least one Agent or SuperAgent".to_string(),
        ));
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_agent() {
        let content = "kind: Agent\nname: test";
        let result = validate_contract(content);
        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_validate_valid_superagent() {
        let content = "kind: SuperAgent\nname: test";
        let result = validate_contract(content);
        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_validate_multidoc() {
        let content = "kind: Agent\nname: a1\n---\nkind: Agent\nname: a2";
        let result = validate_contract(content);
        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_validate_invalid_yaml() {
        let content = "invalid: yaml: content: [";
        let result = validate_contract(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty() {
        let content = "";
        let result = validate_contract(content);
        assert!(result.is_err());
    }
}
