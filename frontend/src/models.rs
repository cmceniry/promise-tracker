use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Generates a random 16-character alphanumeric ID
pub fn generate_id() -> String {
    use js_sys::Math;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut result = String::with_capacity(16);
    for _ in 0..16 {
        let idx = (Math::random() * CHARS.len() as f64) as usize;
        result.push(CHARS[idx] as char);
    }
    result
}

/// Represents a contract loaded in the frontend
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    /// Unique identifier for this contract
    pub id: String,
    /// The filename of the contract
    pub filename: String,
    /// The YAML content of the contract
    pub content: String,
    /// Validation error message (empty if valid)
    #[serde(default)]
    pub err: String,
    /// Set of simulation names this contract is active in
    pub sims: HashSet<String>,
    /// The original server path (for tracking renames/diffs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_path: Option<String>,
}

impl Contract {
    /// Create a new contract with the given filename and content
    pub fn new(filename: String, content: String) -> Self {
        Self {
            id: generate_id(),
            filename,
            content,
            err: String::new(),
            sims: HashSet::new(),
            server_path: None,
        }
    }

    /// Create a new contract with default simulations enabled
    pub fn with_default_sims(filename: String, content: String, simulations: &[String]) -> Self {
        Self {
            id: generate_id(),
            filename,
            content,
            err: String::new(),
            sims: simulations.iter().cloned().collect(),
            server_path: None,
        }
    }

    /// Check if this contract is active in a given simulation
    pub fn is_active_in(&self, sim: &str) -> bool {
        self.sims.contains(sim)
    }

    /// Toggle whether this contract is active in a simulation
    pub fn toggle_sim(&mut self, sim: &str) {
        if self.sims.contains(sim) {
            self.sims.remove(sim);
        } else {
            self.sims.insert(sim.to_string());
        }
    }
}
