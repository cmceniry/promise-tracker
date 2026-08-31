use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

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

/// True when the entry's own name starts with a dot (`.git`, `.notes.yaml`).
/// Hidden entries are never treated as contracts or browsable directories.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// Where `path` really lives, or `None` when it does not resolve (a broken
/// symlink) or resolves outside the base directory.
///
/// `base_dir` must already be canonical, which `Storage::new` guarantees.
fn resolve_within(path: &Path, base_dir: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    canonical.starts_with(base_dir).then_some(canonical)
}

/// Canonical path of every directory passed through on the way to `relative`,
/// starting at the base directory and ending at `relative` itself.
///
/// Symlinks make a directory reachable in more than one shape - `sub/loop -> ..`,
/// or a pair of sibling directories linking to each other - so a loop is a path
/// whose target already appears earlier in this chain.
fn canonical_chain(base_dir: &Path, relative: Option<&str>) -> Vec<PathBuf> {
    let mut chain = vec![base_dir.to_path_buf()];
    let Some(relative) = relative else {
        return chain;
    };

    let mut current = base_dir.to_path_buf();
    for segment in relative.split('/').filter(|segment| !segment.is_empty()) {
        current.push(segment);
        match current.canonicalize() {
            Ok(canonical) => {
                chain.push(canonical.clone());
                current = canonical;
            }
            Err(_) => break,
        }
    }

    chain
}

impl Storage {
    /// Create a new Storage instance with the given base directory
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir
            .as_ref()
            .canonicalize()
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
        let mut visiting = HashSet::from([base_dir.clone()]);
        self.scan_directory_recursive(&base_dir, &base_dir, &mut visiting)?;
        Ok(())
    }

    /// Recursively scan a directory for YAML files
    ///
    /// `visiting` holds the canonical path of every directory on the way down
    /// to `current_dir`. A symlink resolving to one of those closes a loop and
    /// is skipped; a symlink to a directory elsewhere in the tree is not a loop
    /// and is scanned under its own name, so listings and loads agree on it.
    fn scan_directory_recursive(
        &mut self,
        current_dir: &Path,
        base_dir: &Path,
        visiting: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        // Backstop for a chain of symlinked diamonds, which multiplies paths
        // without ever repeating one on the way down.
        const MAX_DEPTH: usize = 64;
        if visiting.len() > MAX_DEPTH {
            warn!("Skipping directory nested deeper than {}: {:?}", MAX_DEPTH, current_dir);
            return Ok(());
        }

        let entries = std::fs::read_dir(current_dir)
            .with_context(|| format!("Failed to read directory: {:?}", current_dir))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if is_hidden(&path) {
                continue;
            }

            let Some(canonical) = resolve_within(&path, base_dir) else {
                // Only symlinks can fail to resolve or land outside the base
                // directory, and either case is worth saying out loud once.
                if entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                    warn!(
                        "Skipping symlink that is broken or points outside the base directory: {:?}",
                        path
                    );
                }
                continue;
            };

            if path.is_dir() {
                if !visiting.insert(canonical.clone()) {
                    warn!("Skipping symlink that loops back on itself: {:?}", path);
                    continue;
                }
                self.scan_directory_recursive(&path, base_dir, visiting)?;
                visiting.remove(&canonical);
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
        let canonical_target = target_dir
            .canonicalize()
            .context("Failed to canonicalize target directory")?;

        if !canonical_target.starts_with(&self.base_dir) {
            anyhow::bail!("Path is outside base directory");
        }

        if !canonical_target.is_dir() {
            anyhow::bail!("Path is not a directory: {:?}", canonical_target);
        }

        // Everything walked through to get here. A client following listings
        // must never be handed an entry that leads back into this chain.
        let chain = canonical_chain(
            &self.base_dir,
            dir_path.map(|path| {
                urlencoding::decode(path)
                    .map(|decoded| decoded.to_string())
                    .unwrap_or_else(|_| path.to_string())
            }).as_deref(),
        );

        if chain[..chain.len().saturating_sub(1)].contains(&canonical_target) {
            anyhow::bail!("Symlink loop detected at: {:?}", canonical_target);
        }

        let mut entries = Vec::new();
        let dir_entries = std::fs::read_dir(&canonical_target)
            .with_context(|| format!("Failed to read directory: {:?}", canonical_target))?;

        for entry in dir_entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if is_hidden(&path) {
                continue;
            }

            // Resolve rather than lstat, so a symlinked contract or directory
            // is listed exactly as the scanner registers it. Broken links and
            // links out of the tree drop out here.
            let Some(canonical) = resolve_within(&path, &self.base_dir) else {
                continue;
            };

            // A link back into the chain closes a loop. Leaving it out beats
            // offering a directory that errors the moment it is opened.
            if chain.contains(&canonical) {
                continue;
            }

            let metadata = std::fs::metadata(&path).context("Failed to read entry metadata")?;

            // Get relative path from base_dir
            let relative_path = path
                .strip_prefix(&self.base_dir)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string());

            if relative_path.is_some() {
                if metadata.is_dir() {
                    entries.push(DirectoryEntry {
                        name: path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string(),
                        entry_type: EntryType::Directory,
                    });
                } else if metadata.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "yaml" || ext == "yml" {
                            entries.push(DirectoryEntry {
                                name: path
                                    .file_name()
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

    /// Load a contract by its ID (relative path)
    pub fn load_contract(&self, contract_id: &str) -> Result<String> {
        let absolute_path = self
            .contracts
            .get(contract_id)
            .ok_or_else(|| anyhow::anyhow!("Contract not found: {}", contract_id))?;

        std::fs::read_to_string(absolute_path)
            .with_context(|| format!("Failed to read contract file: {:?}", absolute_path))
    }

    /// Save a contract to the file system
    /// Creates directories as needed
    pub fn save_contract(&mut self, contract_id: &str, content: &str) -> Result<()> {
        // Validate path before creating directories - reject any use of ..
        let contract_path = PathBuf::from(contract_id);
        for component in contract_path.components() {
            if matches!(component, std::path::Component::ParentDir)
                || matches!(component, std::path::Component::CurDir)
            {
                anyhow::bail!(
                    "Path traversal not allowed: '..' is not permitted in contract paths"
                );
            }

            // Writing a dot-file or dot-directory would create something the
            // scanner and listings deliberately ignore.
            if let std::path::Component::Normal(name) = component {
                if name.to_str().is_some_and(|n| n.starts_with('.')) {
                    anyhow::bail!(
                        "Hidden paths not allowed: '{}' starts with '.'",
                        name.to_string_lossy()
                    );
                }
            }
        }

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

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Scratch directory that removes itself when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unique = format!(
                "promise-tracker-storage-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).expect("create temp dir");
            TempDir(path)
        }

        fn write(&self, relative: &str, content: &str) {
            let target = self.0.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
            std::fs::write(target, content).expect("write file");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_skips_hidden_files_and_directories() {
        let dir = TempDir::new();
        dir.write("visible.yaml", "kind: Agent\n");
        dir.write("nested/also_visible.yaml", "kind: Agent\n");
        dir.write(".hidden.yaml", "kind: Agent\n");
        dir.write(".hiddendir/inside.yaml", "kind: Agent\n");
        dir.write("nested/.hidden_too.yaml", "kind: Agent\n");

        let storage = Storage::new(&dir.0).expect("storage");

        assert_eq!(
            storage.list_contracts(),
            vec![
                "nested/also_visible.yaml".to_string(),
                "visible.yaml".to_string(),
            ]
        );
    }

    #[test]
    fn list_directory_omits_hidden_entries() {
        let dir = TempDir::new();
        dir.write("visible.yaml", "kind: Agent\n");
        dir.write(".hidden.yaml", "kind: Agent\n");
        dir.write("plain/keep.yaml", "kind: Agent\n");
        dir.write(".hiddendir/inside.yaml", "kind: Agent\n");

        let storage = Storage::new(&dir.0).expect("storage");
        let names: Vec<String> = storage
            .list_directory(None)
            .expect("listing")
            .into_iter()
            .map(|entry| entry.name)
            .collect();

        assert_eq!(names, vec!["plain".to_string(), "visible.yaml".to_string()]);
    }

    #[cfg(unix)]
    fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) {
        std::os::unix::fs::symlink(original, link).expect("create symlink");
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_are_scanned_and_listed_alike() {
        let dir = TempDir::new();
        dir.write("real/inside.yaml", "kind: Agent\n");
        dir.write("plain.yaml", "kind: Agent\n");
        symlink("real", dir.0.join("linkdir"));
        symlink("plain.yaml", dir.0.join("linkfile.yaml"));

        let storage = Storage::new(&dir.0).expect("storage");

        // The scanner reaches contracts through both links...
        assert_eq!(
            storage.list_contracts(),
            vec![
                "linkdir/inside.yaml".to_string(),
                "linkfile.yaml".to_string(),
                "plain.yaml".to_string(),
                "real/inside.yaml".to_string(),
            ]
        );

        // ...and the listing shows the same links, rather than hiding entries
        // that are nonetheless loadable.
        let names: Vec<String> = storage
            .list_directory(None)
            .expect("listing")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "linkdir".to_string(),
                "linkfile.yaml".to_string(),
                "plain.yaml".to_string(),
                "real".to_string(),
            ]
        );
    }

    #[test]
    #[cfg(unix)]
    fn symlink_loop_is_walked_once_and_refused_by_listings() {
        let dir = TempDir::new();
        dir.write("sub/x.yaml", "kind: Agent\n");
        symlink("..", dir.0.join("sub/loop"));

        let storage = Storage::new(&dir.0).expect("storage");

        // Without the visited set this registers the same file once per level
        // the platform is willing to resolve.
        assert_eq!(storage.list_contracts(), vec!["sub/x.yaml".to_string()]);

        // The loop entry is left out of the listing, so a client walking the
        // tree is never offered the way back in...
        let names: Vec<String> = storage
            .list_directory(Some("sub"))
            .expect("listing")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["x.yaml".to_string()]);

        // ...and asking for it directly is refused too.
        assert!(storage.list_directory(Some("sub/loop")).is_err());
        assert!(storage.list_directory(Some("sub/loop/sub")).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn mutually_linked_directories_terminate() {
        // Neither link points at an ancestor of itself, so only tracking the
        // descent path catches this: a/to_b/to_a/to_b/... is walkable forever.
        let dir = TempDir::new();
        dir.write("a/x.yaml", "kind: Agent\n");
        dir.write("b/y.yaml", "kind: Agent\n");
        symlink(dir.0.join("b"), dir.0.join("a/to_b"));
        symlink(dir.0.join("a"), dir.0.join("b/to_a"));

        let storage = Storage::new(&dir.0).expect("storage");

        assert_eq!(
            storage.list_contracts(),
            vec![
                "a/to_b/y.yaml".to_string(),
                "a/x.yaml".to_string(),
                "b/to_a/x.yaml".to_string(),
                "b/y.yaml".to_string(),
            ]
        );

        // The second hop back is where the loop closes, so it is neither
        // listed nor reachable.
        let names: Vec<String> = storage
            .list_directory(Some("a/to_b"))
            .expect("listing")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["y.yaml".to_string()]);
        assert!(storage.list_directory(Some("a/to_b/to_a")).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_out_of_the_tree_and_broken_links_are_skipped() {
        let dir = TempDir::new();
        dir.write("base/ok.yaml", "kind: Agent\n");
        dir.write("outside/secret.yaml", "kind: Agent\n");
        symlink(dir.0.join("outside/secret.yaml"), dir.0.join("base/escape.yaml"));
        symlink(dir.0.join("outside"), dir.0.join("base/escapedir"));
        symlink("nowhere.yaml", dir.0.join("base/broken.yaml"));

        let storage = Storage::new(dir.0.join("base")).expect("storage");

        assert_eq!(storage.list_contracts(), vec!["ok.yaml".to_string()]);

        let names: Vec<String> = storage
            .list_directory(None)
            .expect("listing")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["ok.yaml".to_string()]);
    }

    #[test]
    fn save_contract_rejects_hidden_paths() {
        let dir = TempDir::new();
        let mut storage = Storage::new(&dir.0).expect("storage");

        assert!(storage.save_contract(".hidden.yaml", "kind: Agent\n").is_err());
        assert!(storage
            .save_contract(".hiddendir/inside.yaml", "kind: Agent\n")
            .is_err());
        assert!(storage
            .save_contract("nested/.hidden.yaml", "kind: Agent\n")
            .is_err());
        assert!(storage.save_contract("nested/ok.yaml", "kind: Agent\n").is_ok());

        // A rejected write must not leave the directory behind either.
        assert!(!dir.0.join(".hiddendir").exists());
    }
}
