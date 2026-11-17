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

/// Whether a card follows its file on the server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowState {
    /// An ordinary, locally editable card.
    #[default]
    Off,
    /// Following its own `server_path`.
    Direct,
    /// Following because a followed server directory contains it.
    ViaDir,
}

/// A server copy of a followed contract that failed validation. It is kept
/// beside the card's `content` (the last good version) rather than in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingServerCopy {
    /// The raw server text
    pub text: String,
    /// Why it is invalid
    pub err: String,
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
    /// Whether this card follows the server copy (read-only while it does)
    #[serde(default)]
    pub follow: FollowState,
    /// The latest server copy, when it is invalid and `content` holds the
    /// last good version instead
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingServerCopy>,
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
            follow: FollowState::Off,
            pending: None,
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
            follow: FollowState::Off,
            pending: None,
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
