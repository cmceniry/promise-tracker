use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Type of directory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Directory,
    Contract,
}

/// A directory entry (contract or subdirectory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
}

/// Storage manager for contracts stored in the file system
pub struct Storage {
    base_dir: PathBuf,
    contracts: HashMap<String, PathBuf>, // relative_path -> absolute_path
}

impl Storage {
    /// Create a new Storage instance with the given base directory
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir.as_ref().canonicalize()
            .context("Failed to canonicalize base directory")?;
        
        if !base_dir.is_dir() {
            anyhow::bail!("Base directory is not a directory: {:?}", base_dir);
        }

        let mut storage = Storage {
            base_dir: base_dir.clone(),
            contracts: HashMap::new(),
        };

        storage.scan_directory()?;
        Ok(storage)
    }

    /// Scan the base directory recursively for YAML files
    pub fn scan_directory(&mut self) -> Result<()> {
        self.contracts.clear();
        let base_dir = self.base_dir.clone();
        self.scan_directory_recursive(&base_dir, &base_dir)?;
        Ok(())
    }

    /// Recursively scan a directory for YAML files
    fn scan_directory_recursive(
        &mut self,
        current_dir: &Path,
        base_dir: &Path,
    ) -> Result<()> {
        let entries = std::fs::read_dir(current_dir)
            .with_context(|| format!("Failed to read directory: {:?}", current_dir))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.is_dir() {
                self.scan_directory_recursive(&path, base_dir)?;
            } else if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "yaml" || ext == "yml" {
                        if let Ok(relative_path) = path.strip_prefix(base_dir) {
                            let relative_str = relative_path.to_string_lossy().to_string();
                            self.contracts.insert(relative_str, path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// List contents of a directory (non-recursive)
    /// Returns a list of directory entries with their types
    pub fn list_directory(&self, dir_path: Option<&str>) -> Result<Vec<DirectoryEntry>> {
        let target_dir = if let Some(path) = dir_path {
            let decoded_path = urlencoding::decode(path)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());
            self.base_dir.join(&decoded_path)
        } else {
            self.base_dir.clone()
        };

        // Ensure the path is within base_dir for security
        let canonical_target = target_dir.canonicalize()
            .context("Failed to canonicalize target directory")?;
        
        if !canonical_target.starts_with(&self.base_dir) {
            anyhow::bail!("Path is outside base directory");
        }

        if !canonical_target.is_dir() {
            anyhow::bail!("Path is not a directory: {:?}", canonical_target);
        }

        let mut entries = Vec::new();
        let dir_entries = std::fs::read_dir(&canonical_target)
            .with_context(|| format!("Failed to read directory: {:?}", canonical_target))?;

        for entry in dir_entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();
            let metadata = entry.metadata()
                .context("Failed to read entry metadata")?;

            // Get relative path from base_dir
            let relative_path = path.strip_prefix(&self.base_dir)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string());

            if relative_path.is_some() {
                if metadata.is_dir() {
                    entries.push(DirectoryEntry {
                        name: path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string(),
                        entry_type: EntryType::Directory,
                    });
                } else if metadata.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "yaml" || ext == "yml" {
                            entries.push(DirectoryEntry {
                                name: path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string(),
                                entry_type: EntryType::Contract,
                            });
                        }
                    }
                }
            }
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Get all contract IDs (relative paths) - kept for backward compatibility
    pub fn list_contracts(&self) -> Vec<String> {
        let mut contracts: Vec<String> = self.contracts.keys().cloned().collect();
        contracts.sort();
        contracts
    }

    /// Check if a contract exists
    pub fn has_contract(&self, contract_id: &str) -> bool {
        self.contracts.contains_key(contract_id)
    }

    /// Load a contract by its ID (relative path)
    pub fn load_contract(&self, contract_id: &str) -> Result<String> {
        let absolute_path = self.contracts.get(contract_id)
            .ok_or_else(|| anyhow::anyhow!("Contract not found: {}", contract_id))?;
        
        std::fs::read_to_string(absolute_path)
            .with_context(|| format!("Failed to read contract file: {:?}", absolute_path))
    }

    /// Save a contract to the file system
    /// Creates directories as needed
    pub fn save_contract(&mut self, contract_id: &str, content: &str) -> Result<()> {
        let target_path = self.base_dir.join(contract_id);
        
        // Create parent directories if they don't exist
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Write the file
        std::fs::write(&target_path, content)
            .with_context(|| format!("Failed to write contract file: {:?}", target_path))?;

        // Update the contracts map
        self.contracts.insert(contract_id.to_string(), target_path);

        Ok(())
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

