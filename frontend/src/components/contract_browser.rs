use gloo_net::http::Request;
use leptos::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

/// Directory entry from the API
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
}

/// Encode a path for use in API URLs
fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| js_sys::encode_uri_component(segment).as_string().unwrap())
        .collect::<Vec<_>>()
        .join("/")
}

/// Fetch directory listing from API
async fn fetch_directory_listing(path: Option<&str>) -> Result<Vec<DirectoryEntry>, String> {
    let url = match path {
        Some(p) if !p.is_empty() => format!("/contracts/{}", encode_path(p)),
        _ => "/contracts".to_string(),
    };

    let response = Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!(
            "Failed to fetch directory: {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Vec<DirectoryEntry>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch contract content from API
async fn fetch_contract(contract_id: &str) -> Result<String, String> {
    let url = format!("/contracts/{}", encode_path(contract_id));

    let response = Request::get(&url)
        .header("Accept", "application/x-yaml")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!(
            "Failed to fetch contract: {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Read error: {}", e))
}

/// Tree browser modal for loading contracts from API server.
#[component]
pub fn ContractBrowser(
    show: ReadSignal<bool>,
    #[prop(into)] on_hide: Callback<()>,
    #[prop(into)] on_select_contract: Callback<(String, String, String)>, // (id, filename, content)
    #[prop(into)] downloaded_contract_paths: Signal<HashSet<String>>,
) -> impl IntoView {
    // State signals
    let (expanded_paths, set_expanded_paths) = signal::<HashSet<String>>(HashSet::new());
    let (loaded_children, set_loaded_children) =
        signal::<HashMap<String, Vec<DirectoryEntry>>>(HashMap::new());
    let (loading_paths, set_loading_paths) = signal::<HashSet<String>>(HashSet::new());
    let (root_entries, set_root_entries) = signal::<Vec<DirectoryEntry>>(Vec::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let (downloading_contracts, set_downloading_contracts) =
        signal::<HashSet<String>>(HashSet::new());
    let (selected_paths, set_selected_paths) = signal::<HashSet<String>>(HashSet::new());
    // Set while selected directories are being walked on the server, before any
    // download has started and so before downloading_contracts says anything.
    let (resolving, set_resolving) = signal(false);

    // Ref for focusing the modal
    let modal_ref = NodeRef::<leptos::html::Div>::new();

    // Use the signal for downloaded paths
    let downloaded_paths = downloaded_contract_paths;

    let is_visible = move || show.get();

    // Load root directory when modal opens
    Effect::new(move |_| {
        if show.get() {
            // Reset state
            set_expanded_paths.set(HashSet::new());
            set_loaded_children.set(HashMap::new());
            set_loading_paths.set(HashSet::new());
            set_downloading_contracts.set(HashSet::new());
            set_selected_paths.set(HashSet::new());
            set_resolving.set(false);
            set_error.set(None);

            // Load root directory
            set_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_directory_listing(None).await {
                    Ok(entries) => {
                        set_root_entries.set(entries);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_root_entries.set(Vec::new());
                    }
                }
                set_loading.set(false);
            });

            // Focus the modal for keyboard events
            if let Some(el) = modal_ref.get() {
                let _ = el.unchecked_ref::<HtmlElement>().focus();
            }
        }
    });

    // Handle close
    let handle_close = move |_| {
        set_expanded_paths.set(HashSet::new());
        set_loaded_children.set(HashMap::new());
        set_loading_paths.set(HashSet::new());
        set_root_entries.set(Vec::new());
        set_downloading_contracts.set(HashSet::new());
        set_selected_paths.set(HashSet::new());
        set_error.set(None);
        on_hide.run(());
    };

    // Toggle directory expansion
    let toggle_expand = move |path: String| {
        let is_expanded = expanded_paths.get().contains(&path);

        if is_expanded {
            // Collapse
            set_expanded_paths.update(|paths| {
                paths.remove(&path);
            });
        } else {
            // Expand - load children if not already loaded
            set_expanded_paths.update(|paths| {
                paths.insert(path.clone());
            });

            // Check if already loaded
            if !loaded_children.get().contains_key(&path) {
                // Mark as loading
                set_loading_paths.update(|paths| {
                    paths.insert(path.clone());
                });

                let path_clone = path.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_directory_listing(Some(&path_clone)).await {
                        Ok(entries) => {
                            set_loaded_children.update(|children| {
                                children.insert(path_clone.clone(), entries);
                            });
                        }
                        Err(e) => {
                            set_error.set(Some(e));
                        }
                    }
                    set_loading_paths.update(|paths| {
                        paths.remove(&path_clone);
                    });
                });
            }
        }
    };

    // Download a contract
    let download_contract = move |path: String, _name: String| {
        // Check if already downloading or downloaded
        if downloading_contracts.get().contains(&path) || downloaded_paths.get().contains(&path) {
            return;
        }

        // Mark as downloading
        set_downloading_contracts.update(|paths| {
            paths.insert(path.clone());
        });
        set_error.set(None);

        let path_clone = path.clone();
        let on_select = on_select_contract.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_contract(&path_clone).await {
                Ok(content) => {
                    // Use full path as filename to match subsequent server calls
                    on_select.run((path_clone.clone(), path_clone.clone(), content));
                }
                Err(e) => {
                    set_error.update(|err| {
                        let msg = format!("{}: {}", path_clone, e);
                        *err = Some(match err.take() {
                            Some(prev) => format!("{}; {}", prev, msg),
                            None => msg,
                        });
                    });
                }
            }
            set_downloading_contracts.update(|paths| {
                paths.remove(&path_clone);
            });
        });
    };

    // Dismiss error
    let dismiss_error = move |_| {
        set_error.set(None);
    };

    // Report a problem without discarding one already on screen.
    let append_error = move |message: String| {
        set_error.update(|err| {
            *err = Some(match err.take() {
                Some(previous) => format!("{}; {}", previous, message),
                None => message,
            });
        });
    };

    // Fetch contracts and hand them to the carder.
    let start_downloads = {
        let on_select = on_select_contract.clone();
        move |paths: Vec<String>| {
            if paths.is_empty() {
                return;
            }

            set_downloading_contracts.update(|downloading| {
                downloading.extend(paths.iter().cloned());
            });

            for path in paths {
                let on_select = on_select.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_contract(&path).await {
                        Ok(content) => {
                            on_select.run((path.clone(), path.clone(), content));
                        }
                        Err(e) => append_error(format!("{}: {}", path, e)),
                    }

                    set_downloading_contracts.update(|downloading| {
                        downloading.remove(&path);
                    });
                });
            }
        }
    };

    // Handle adding the selection: contracts are known already, directories get
    // walked on the server so that a folder checked without ever being expanded
    // still brings in everything underneath it.
    let handle_add_selected = {
        let start_downloads = start_downloads.clone();
        move |_| {
            let selected = selected_paths.get_untracked();
            if selected.is_empty() {
                return;
            }

            let loaded = loaded_children.get_untracked();
            let roots = root_entries.get_untracked();

            let (directories, contracts): (Vec<String>, Vec<String>) = selected
                .into_iter()
                .partition(|path| is_directory_path(path, &roots, &loaded));

            set_selected_paths.set(HashSet::new());

            if directories.is_empty() {
                start_downloads(filter_addable(
                    contracts,
                    &downloaded_paths.get_untracked(),
                    &downloading_contracts.get_untracked(),
                ));
                return;
            }

            set_resolving.set(true);
            let start_downloads = start_downloads.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut found = contracts;
                for directory in directories {
                    let (contracts, errors) = collect_contracts_under(&directory).await;
                    found.extend(contracts);
                    for error in errors {
                        append_error(error);
                    }
                }

                set_resolving.set(false);
                start_downloads(filter_addable(
                    found,
                    &downloaded_paths.get_untracked(),
                    &downloading_contracts.get_untracked(),
                ));
            });
        }
    };

    // Size of the current selection: exact once every selected directory has
    // been walked, a floor with a "+" while any of them is still unexplored.
    let selection_summary = move || {
        let selected = selected_paths.get();
        if selected.is_empty() {
            return (0usize, false);
        }

        let loaded = loaded_children.get();
        let downloaded = downloaded_paths.get();
        let downloading = downloading_contracts.get();

        let mut known = Vec::new();
        collect_known_contracts(&root_entries.get(), None, &loaded, &mut known);

        let count = known
            .iter()
            .filter(|path| {
                !downloaded.contains(*path)
                    && !downloading.contains(*path)
                    && is_selected(path, &selected)
            })
            .count();

        let roots = root_entries.get();
        let incomplete = selected
            .iter()
            .any(|path| is_directory_path(path, &roots, &loaded) && !subtree_loaded(path, &loaded));

        (count, incomplete)
    };

    // Handle escape key to close modal
    let handle_keydown = {
        let on_hide = on_hide.clone();
        move |ev: KeyboardEvent| {
            if ev.key() == "Escape" {
                ev.prevent_default();
                set_expanded_paths.set(HashSet::new());
                set_loaded_children.set(HashMap::new());
                set_loading_paths.set(HashSet::new());
                set_root_entries.set(Vec::new());
                set_downloading_contracts.set(HashSet::new());
                set_selected_paths.set(HashSet::new());
                set_error.set(None);
                on_hide.run(());
            }
        }
    };

    view! {
        <div
            class="modal"
            class:show=is_visible
            style:display=move || if is_visible() { "block" } else { "none" }
            tabindex="-1"
            on:keydown=handle_keydown
            node_ref=modal_ref
        >
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">"Load Contract from Server"</h5>
                        <button
                            type="button"
                            class="btn-close"
                            on:click=handle_close
                        ></button>
                    </div>
                    <div class="modal-body" style="max-height: 60vh; overflow-y: auto;">
                        // Error alert
                        <Show when=move || error.get().is_some()>
                            <div class="alert alert-danger alert-dismissible fade show" role="alert">
                                {move || error.get().unwrap_or_default()}
                                <button
                                    type="button"
                                    class="btn-close"
                                    on:click=dismiss_error
                                ></button>
                            </div>
                        </Show>

                        // Loading spinner
                        <Show when=move || loading.get()>
                            <div style="text-align: center; padding: 2rem;">
                                <div class="spinner-border" role="status">
                                    <span class="visually-hidden">"Loading..."</span>
                                </div>
                            </div>
                        </Show>

                        // Tree view
                        <Show when=move || !loading.get()>
                            <ul class="list-group">
                                {move || {
                                    let entries = root_entries.get();
                                    if entries.is_empty() {
                                        view! {
                                            <li class="list-group-item">
                                                "No contracts or directories found."
                                            </li>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <For
                                                each=move || root_entries.get()
                                                key=|entry| entry.name.clone()
                                                children=move |entry| {
                                                    let path = entry.name.clone();
                                                    view! {
                                                        <TreeNode
                                                            entry=entry
                                                            path=path
                                                            level=0
                                                            expanded_paths=expanded_paths
                                                            loaded_children=loaded_children
                                                            loading_paths=loading_paths
                                                            on_toggle_expand=toggle_expand.clone()
                                                            on_download_contract=download_contract.clone()
                                                            downloaded_paths=downloaded_paths
                                                            downloading_contracts=downloading_contracts
                                                            selected_paths=selected_paths
                                                            set_selected_paths=set_selected_paths
                                                        />
                                                    }
                                                }
                                            />
                                        }.into_any()
                                    }
                                }}
                            </ul>
                        </Show>
                    </div>
                    <div class="modal-footer">
                        <button
                            type="button"
                            class="btn btn-primary"
                            disabled=move || selected_paths.get().is_empty() ||
                                            resolving.get() ||
                                            !downloading_contracts.get().is_empty()
                            on:click=handle_add_selected
                        >
                            {move || {
                                if resolving.get() {
                                    return "Finding contracts...".to_string();
                                }
                                let (count, incomplete) = selection_summary();
                                match (count, incomplete) {
                                    (0, false) => "Add Selected".to_string(),
                                    // A directory nobody expanded could hold
                                    // anything, so the count is a floor.
                                    (count, true) => format!("Add Selected ({}+)", count),
                                    (count, false) => format!("Add Selected ({})", count),
                                }
                            }}
                        </button>
                        <button
                            type="button"
                            class="btn btn-secondary"
                            on:click=handle_close
                        >
                            "Close"
                        </button>
                    </div>
                </div>
            </div>
        </div>
        // Modal backdrop
        <Show when=is_visible>
            <div class="modal-backdrop fade show"></div>
        </Show>
    }
}

// ---------------------------------------------------------------------------
// Selection model
//
// The selection set holds contract paths and directory paths alike. A directory
// in the set means "every available contract under here", including ones the
// tree has not fetched yet - that is what lets a collapsed directory be checked
// and resolved later, when Add Selected walks it on the server.
//
// The set is kept minimal: selecting a directory drops explicit selections
// underneath it, and unselecting something inside an implicitly selected
// directory first pushes that selection down onto the children the tree knows.
// ---------------------------------------------------------------------------

/// Path of `name` inside `dir`.
fn child_path(dir: &str, name: &str) -> String {
    format!("{}/{}", dir, name)
}

/// Ancestor directory paths of `path`, innermost first.
fn ancestor_paths(path: &str) -> Vec<&str> {
    let mut ancestors = Vec::new();
    let mut rest = path;
    while let Some(index) = rest.rfind('/') {
        rest = &rest[..index];
        ancestors.push(rest);
    }
    ancestors
}

/// True when `path` sits underneath `root`.
fn is_descendant_of(path: &str, root: &str) -> bool {
    path.len() > root.len() + 1
        && path.starts_with(root)
        && path.as_bytes()[root.len()] == b'/'
}

/// True when `path` is selected outright or covered by a selected ancestor.
fn is_selected(path: &str, selected: &HashSet<String>) -> bool {
    selected.contains(path)
        || ancestor_paths(path)
            .iter()
            .any(|ancestor| selected.contains(*ancestor))
}

/// Replace the implicit selection of every selected ancestor of `path` with
/// explicit selections of the children the tree knows about, so that dropping
/// `path` leaves its siblings and cousins selected.
///
/// Contracts already in the carder are left out: they are not selectable, and a
/// checked-looking disabled row would be a lie.
fn push_selection_down(
    selected: &mut HashSet<String>,
    path: &str,
    loaded_children: &HashMap<String, Vec<DirectoryEntry>>,
    downloaded: &HashSet<String>,
    downloading: &HashSet<String>,
) {
    let mut ancestors = ancestor_paths(path);
    ancestors.reverse(); // outermost first, each step handing the selection inward

    for ancestor in ancestors {
        if !selected.remove(ancestor) {
            continue;
        }
        let Some(children) = loaded_children.get(ancestor) else {
            continue;
        };
        for child in children {
            let path = child_path(ancestor, &child.name);
            if child.entry_type == "directory"
                || (!downloaded.contains(&path) && !downloading.contains(&path))
            {
                selected.insert(path);
            }
        }
    }
}

/// Select `path`, dropping selections underneath that it now covers.
fn select_path(selected: &mut HashSet<String>, path: &str) {
    selected.retain(|existing| !is_descendant_of(existing, path));
    selected.insert(path.to_string());
}

/// Unselect `path`, whether it was selected outright or through an ancestor.
fn unselect_path(
    selected: &mut HashSet<String>,
    path: &str,
    loaded_children: &HashMap<String, Vec<DirectoryEntry>>,
    downloaded: &HashSet<String>,
    downloading: &HashSet<String>,
) {
    push_selection_down(selected, path, loaded_children, downloaded, downloading);
    selected.remove(path);
    selected.retain(|existing| !is_descendant_of(existing, path));
}

/// How much of a directory's contents the selection covers.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Coverage {
    /// Not fetched yet, so its contents are anybody's guess.
    Unknown,
    /// Fetched, and holding nothing that could be added.
    Empty,
    /// Everything available underneath is selected.
    Covered,
    /// Some of it is selected, or some of it is still unknown.
    Partial,
}

/// Whether everything available under `dir` is selected.
///
/// This is what re-checks a directory once its last file is ticked, and clears
/// it again the moment one is unticked. An unexpanded subdirectory nobody
/// selected leaves the answer `Partial`: the tree cannot claim to cover what it
/// has never seen.
fn coverage(
    dir: &str,
    loaded_children: &HashMap<String, Vec<DirectoryEntry>>,
    selected: &HashSet<String>,
    downloaded: &HashSet<String>,
    downloading: &HashSet<String>,
) -> Coverage {
    let Some(children) = loaded_children.get(dir) else {
        return Coverage::Unknown;
    };

    let mut saw_content = false;
    for child in children {
        let path = child_path(dir, &child.name);
        let child_coverage = if child.entry_type == "directory" {
            if selected.contains(&path) {
                Coverage::Covered
            } else {
                coverage(&path, loaded_children, selected, downloaded, downloading)
            }
        } else if downloaded.contains(&path) || downloading.contains(&path) {
            Coverage::Empty
        } else if selected.contains(&path) {
            Coverage::Covered
        } else {
            Coverage::Partial
        };

        match child_coverage {
            Coverage::Empty => {}
            Coverage::Covered => saw_content = true,
            Coverage::Partial | Coverage::Unknown => return Coverage::Partial,
        }
    }

    if saw_content {
        Coverage::Covered
    } else {
        Coverage::Empty
    }
}

/// True when every directory under `dir` has been fetched, so the size of a
/// selection rooted here is known rather than estimated.
fn subtree_loaded(dir: &str, loaded_children: &HashMap<String, Vec<DirectoryEntry>>) -> bool {
    let Some(children) = loaded_children.get(dir) else {
        return false;
    };
    children
        .iter()
        .filter(|child| child.entry_type == "directory")
        .all(|child| subtree_loaded(&child_path(dir, &child.name), loaded_children))
}

/// Every contract path the tree currently knows about.
fn collect_known_contracts(
    entries: &[DirectoryEntry],
    prefix: Option<&str>,
    loaded_children: &HashMap<String, Vec<DirectoryEntry>>,
    found: &mut Vec<String>,
) {
    for entry in entries {
        let path = match prefix {
            Some(prefix) => child_path(prefix, &entry.name),
            None => entry.name.clone(),
        };
        if entry.entry_type == "directory" {
            if let Some(children) = loaded_children.get(&path) {
                collect_known_contracts(children, Some(&path), loaded_children, found);
            }
        } else {
            found.push(path);
        }
    }
}

/// Whether the tree lists `path` as a directory. Anything selectable has been
/// rendered, so its parent listing is in hand.
fn is_directory_path(
    path: &str,
    root_entries: &[DirectoryEntry],
    loaded_children: &HashMap<String, Vec<DirectoryEntry>>,
) -> bool {
    let (parent, name) = match path.rfind('/') {
        Some(index) => (Some(&path[..index]), &path[index + 1..]),
        None => (None, path),
    };

    let siblings = match parent {
        Some(parent) => loaded_children.get(parent).map(|entries| entries.as_slice()),
        None => Some(root_entries),
    };

    siblings
        .and_then(|entries| entries.iter().find(|entry| entry.name == name))
        .is_some_and(|entry| entry.entry_type == "directory")
}

/// Drop paths that are already in the carder or on their way there, and any
/// duplicate reached through more than one selected directory.
fn filter_addable(
    paths: Vec<String>,
    downloaded: &HashSet<String>,
    downloading: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            !downloaded.contains(path) && !downloading.contains(path) && seen.insert(path.clone())
        })
        .collect()
}

/// Walk a directory on the server and return every contract path beneath it,
/// alongside the subdirectories that could not be read - one unreadable folder
/// should not sink the rest of the add.
///
/// The server refuses to list a path that loops back on itself, so this cannot
/// spin; the depth cap is a backstop against a server that forgets to.
async fn collect_contracts_under(root: &str) -> (Vec<String>, Vec<String>) {
    const MAX_DEPTH: usize = 32;

    let mut contracts = Vec::new();
    let mut errors = Vec::new();
    let mut pending = vec![(root.to_string(), 0usize)];

    while let Some((dir, depth)) = pending.pop() {
        match fetch_directory_listing(Some(&dir)).await {
            Ok(entries) => {
                for entry in entries {
                    let path = child_path(&dir, &entry.name);
                    if entry.entry_type == "directory" {
                        if depth < MAX_DEPTH {
                            pending.push((path, depth + 1));
                        } else {
                            errors.push(format!("{}: nested too deep to follow", path));
                        }
                    } else {
                        contracts.push(path);
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {}", dir, e)),
        }
    }

    (contracts, errors)
}

/// Recursive tree node component
#[component]
fn TreeNode(
    entry: DirectoryEntry,
    path: String,
    level: usize,
    expanded_paths: ReadSignal<HashSet<String>>,
    loaded_children: ReadSignal<HashMap<String, Vec<DirectoryEntry>>>,
    loading_paths: ReadSignal<HashSet<String>>,
    on_toggle_expand: impl Fn(String) + Clone + Send + Sync + 'static,
    on_download_contract: impl Fn(String, String) + Clone + Send + Sync + 'static,
    downloaded_paths: Signal<HashSet<String>>,
    downloading_contracts: ReadSignal<HashSet<String>>,
    selected_paths: ReadSignal<HashSet<String>>,
    set_selected_paths: WriteSignal<HashSet<String>>,
) -> impl IntoView {
    let is_directory = entry.entry_type == "directory";
    let entry_name = entry.name.clone();

    let indent = level * 20;

    // Store path for reactive lookups
    let stored_path = StoredValue::new(path.clone());

    // Create reactive derived signals using stored path.
    //
    // try_get_value everywhere: collapsing an ancestor directory disposes this
    // row via the ancestor's <Show>, but the same expanded_paths write that
    // collapsed it can still deliver one final notification to this row's
    // closures. get_value() on the disposed StoredValue would panic and wedge
    // the whole reactive runtime.
    let is_expanded = move || {
        let paths = expanded_paths.get();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };
    let is_loading = move || {
        let paths = loading_paths.get();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };
    let is_downloaded = move || {
        let paths = downloaded_paths.get();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };
    let is_downloading = move || {
        let paths = downloading_contracts.get();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };

    // Event handlers must read untracked; Leptos warns on .get() outside a
    // reactive tracking context.
    let is_downloaded_untracked = move || {
        let paths = downloaded_paths.get_untracked();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };
    let is_downloading_untracked = move || {
        let paths = downloading_contracts.get_untracked();
        stored_path.try_get_value().is_some_and(|p| paths.contains(&p))
    };

    // Store callbacks for reuse
    let stored_toggle = StoredValue::new(on_toggle_expand.clone());
    let stored_download = StoredValue::new(on_download_contract.clone());

    let handle_directory_click = {
        let p = path.clone();
        move |_| {
            if let Some(toggle) = stored_toggle.try_get_value() {
                toggle(p.clone());
            }
        }
    };

    // Checked when this row is selected outright or covered by a selected
    // ancestor - that is how a directory checked while collapsed reaches rows
    // that only come into existence when it is expanded.
    let is_row_selected = move || {
        let selected = selected_paths.get();
        stored_path
            .try_get_value()
            .is_some_and(|path| is_selected(&path, &selected))
    };

    let set_row_selection = move |select: bool| {
        let Some(path) = stored_path.try_get_value() else {
            return;
        };
        let loaded = loaded_children.get_untracked();
        let downloaded = downloaded_paths.get_untracked();
        let downloading = downloading_contracts.get_untracked();

        set_selected_paths.update(|selected| {
            if select {
                select_path(selected, &path);
            } else {
                unselect_path(selected, &path, &loaded, &downloaded, &downloading);
            }
        });
    };

    // Selection toggle shared by the checkbox and the row click, so clicking
    // anywhere on a contract row behaves like clicking the box itself.
    let toggle_selection = move || {
        if is_downloaded_untracked() || is_downloading_untracked() {
            return;
        }
        let Some(path) = stored_path.try_get_value() else {
            return;
        };
        let selected_now = is_selected(&path, &selected_paths.get_untracked());
        set_row_selection(!selected_now);
    };

    let handle_checkbox_toggle = move |ev: web_sys::MouseEvent| {
        // Without this the row handler fires too and cancels the toggle out.
        ev.stop_propagation();
        toggle_selection();
    };

    // Directories always respond to a click; contracts stop responding once
    // they are downloaded or in flight, matching the disabled checkbox.
    let is_row_interactive = move || is_directory || !(is_downloaded() || is_downloading());

    // A directory is checked when it is selected (outright or through an
    // ancestor), and also when everything available underneath happens to be
    // selected - which is what re-checks it as the last file is ticked and
    // clears it again as soon as one is unticked.
    let directory_checked = move || {
        if !is_directory {
            return false;
        }
        if is_row_selected() {
            return true;
        }
        stored_path.try_get_value().is_some_and(|path| {
            coverage(
                &path,
                &loaded_children.get(),
                &selected_paths.get(),
                &downloaded_paths.get(),
                &downloading_contracts.get(),
            ) == Coverage::Covered
        })
    };

    let handle_directory_checkbox = move |ev: web_sys::MouseEvent| {
        // Keep the click off the row, which would expand or collapse instead.
        ev.stop_propagation();

        let Some(path) = stored_path.try_get_value() else {
            return;
        };
        let selected_now = selected_paths.get_untracked();
        let checked = is_selected(&path, &selected_now)
            || coverage(
                &path,
                &loaded_children.get_untracked(),
                &selected_now,
                &downloaded_paths.get_untracked(),
                &downloading_contracts.get_untracked(),
            ) == Coverage::Covered;

        // Whatever the rows underneath say individually, they follow this box.
        set_row_selection(!checked);
    };

    view! {
        <li
            class="list-group-item"
            class:list-group-item-action=is_row_interactive
            style:cursor=move || if is_row_interactive() { "pointer" } else { "default" }
            style:display="flex"
            style:align-items="center"
            style:gap="0.5rem"
            style:padding-left=format!("{}px", 8 + indent)
            style:opacity=move || if is_downloaded() { "0.6" } else { "1" }
            on:click=move |_| {
                if is_directory {
                    handle_directory_click(());
                } else {
                    toggle_selection();
                }
            }
        >
            // Directory chevron and icon
            {if is_directory {
                view! {
                    <>
                        <span style="flex-shrink: 0; width: 16px;">
                            {move || if is_expanded() { "▼" } else { "▶" }}
                        </span>
                        <span style="flex-shrink: 0;">"📁"</span>
                    </>
                }.into_any()
            } else {
                view! {
                    <span style="flex-shrink: 0;">"📄"</span>
                }.into_any()
            }}

            // Entry name
            <span style:text-decoration=move || if is_downloaded() { "line-through" } else { "none" }
                  style:flex-grow="1">
                {entry_name.clone()}
            </span>

            // Checkbox: per contract, or select-the-whole-subtree per directory.
            // prop:checked rather than the attribute - once a box has been
            // clicked, only the property moves it, and these boxes are driven
            // from state as often as from their own clicks.
            {if !is_directory {
                view! {
                    <input
                        type="checkbox"
                        class="form-check-input"
                        style:cursor="pointer"
                        // A downloaded row is inert, so it never shows the
                        // tick its selected parent would otherwise give it.
                        prop:checked=move || {
                            is_row_selected() && !is_downloaded() && !is_downloading()
                        }
                        disabled=move || is_downloaded() || is_downloading()
                        on:click=handle_checkbox_toggle
                    />
                }.into_any()
            } else {
                view! {
                    <input
                        type="checkbox"
                        class="form-check-input"
                        style:cursor="pointer"
                        title="Select every available contract in this directory, including any not loaded yet"
                        prop:checked=directory_checked
                        on:click=handle_directory_checkbox
                    />
                }.into_any()
            }}

            // Loading spinner
            <Show when=move || is_loading() || is_downloading()>
                <span class="spinner-border spinner-border-sm" style="margin-left: 0.5rem;"></span>
            </Show>
        </li>

        // Children (for expanded directories)
        <Show when=move || is_directory && is_expanded() && !is_loading()>
            {move || {
                let children_map = loaded_children.get();
                let (Some(cp), Some(toggle), Some(download)) = (
                    stored_path.try_get_value(),
                    stored_toggle.try_get_value(),
                    stored_download.try_get_value(),
                ) else {
                    // Row already disposed; render nothing while it unmounts.
                    return view! { <></> }.into_any();
                };
                let children = children_map.get(&cp).cloned().unwrap_or_default();

                if children.is_empty() {
                    view! {
                        <li
                            class="list-group-item"
                            style:padding-left=format!("{}px", 8 + indent + 20)
                            style:font-style="italic"
                            style:color="#666"
                        >
                            "Empty directory"
                        </li>
                    }.into_any()
                } else {
                    let parent_path = cp.clone();

                    view! {
                        <For
                            each=move || {
                                let children_map = loaded_children.get();
                                stored_path
                                    .try_get_value()
                                    .and_then(|pp| children_map.get(&pp).cloned())
                                    .unwrap_or_default()
                            }
                            key=|entry| entry.name.clone()
                            children={
                                let toggle = toggle.clone();
                                let download = download.clone();
                                let parent = parent_path.clone();
                                move |child| {
                                    let child_path = format!("{}/{}", parent, child.name);
                                    view! {
                                        <TreeNode
                                            entry=child
                                            path=child_path
                                            level=level + 1
                                            expanded_paths=expanded_paths
                                            loaded_children=loaded_children
                                            loading_paths=loading_paths
                                            on_toggle_expand=toggle.clone()
                                            on_download_contract=download.clone()
                                            downloaded_paths=downloaded_paths
                                            downloading_contracts=downloading_contracts
                                            selected_paths=selected_paths
                                            set_selected_paths=set_selected_paths
                                        />
                                    }
                                }
                            }
                        />
                    }.into_any()
                }
            }}
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            entry_type: "directory".to_string(),
        }
    }

    fn contract(name: &str) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            entry_type: "contract".to_string(),
        }
    }

    fn roots() -> Vec<DirectoryEntry> {
        vec![dir("alpha"), contract("root.yaml")]
    }

    /// alpha/{a.yaml, sub/{c.yaml}, b.yaml}, every directory expanded.
    fn loaded_tree() -> HashMap<String, Vec<DirectoryEntry>> {
        HashMap::from([
            (
                "alpha".to_string(),
                vec![contract("a.yaml"), dir("sub"), contract("b.yaml")],
            ),
            ("alpha/sub".to_string(), vec![contract("c.yaml")]),
        ])
    }

    fn paths(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn selection_reaches_rows_under_a_selected_directory() {
        let selected = paths(&["alpha"]);

        assert!(is_selected("alpha", &selected));
        assert!(is_selected("alpha/a.yaml", &selected));
        assert!(is_selected("alpha/sub/c.yaml", &selected));
        assert!(!is_selected("root.yaml", &selected));
        // Sharing a prefix is not the same as sitting underneath.
        assert!(!is_selected("alphabet/a.yaml", &selected));
    }

    #[test]
    fn selecting_a_directory_absorbs_the_selections_underneath_it() {
        let mut selected = paths(&["alpha/a.yaml", "alpha/sub", "root.yaml"]);

        select_path(&mut selected, "alpha");

        assert_eq!(selected, paths(&["alpha", "root.yaml"]));
    }

    #[test]
    fn unselecting_inside_a_directory_keeps_everything_else_selected() {
        let mut selected = paths(&["alpha"]);

        unselect_path(
            &mut selected,
            "alpha/sub/c.yaml",
            &loaded_tree(),
            &HashSet::new(),
            &HashSet::new(),
        );

        // alpha's implicit selection is handed down a level at a time, and
        // alpha/sub disappears with its only file.
        assert_eq!(selected, paths(&["alpha/a.yaml", "alpha/b.yaml"]));
    }

    #[test]
    fn pushing_a_selection_down_skips_contracts_already_in_the_carder() {
        let mut selected = paths(&["alpha"]);

        unselect_path(
            &mut selected,
            "alpha/b.yaml",
            &loaded_tree(),
            &paths(&["alpha/a.yaml"]),
            &HashSet::new(),
        );

        assert_eq!(selected, paths(&["alpha/sub"]));
    }

    #[test]
    fn a_directory_covers_itself_only_when_the_subtree_is_accounted_for() {
        let loaded = loaded_tree();
        let none = HashSet::new();
        let all = paths(&["alpha/a.yaml", "alpha/b.yaml", "alpha/sub/c.yaml"]);

        assert_eq!(coverage("alpha", &loaded, &all, &none, &none), Coverage::Covered);
        assert_eq!(
            coverage("alpha", &loaded, &paths(&["alpha/a.yaml"]), &none, &none),
            Coverage::Partial
        );
        assert_eq!(coverage("alpha/unopened", &loaded, &all, &none, &none), Coverage::Unknown);

        // An unexpanded directory could hold anything, so its parent cannot
        // claim to cover it - unless it was selected outright.
        let mut unexpanded = loaded.clone();
        unexpanded.get_mut("alpha").unwrap().push(dir("deep"));
        assert_eq!(
            coverage("alpha", &unexpanded, &all, &none, &none),
            Coverage::Partial
        );

        let mut with_deep = all.clone();
        with_deep.insert("alpha/deep".to_string());
        assert_eq!(
            coverage("alpha", &unexpanded, &with_deep, &none, &none),
            Coverage::Covered
        );
    }

    #[test]
    fn contracts_in_the_carder_do_not_hold_a_directory_back() {
        let loaded = loaded_tree();
        let none = HashSet::new();

        // a and c are already loaded, so selecting b covers what is left.
        assert_eq!(
            coverage(
                "alpha",
                &loaded,
                &paths(&["alpha/b.yaml"]),
                &paths(&["alpha/a.yaml", "alpha/sub/c.yaml"]),
                &none,
            ),
            Coverage::Covered
        );

        // With nothing left to add, the directory is empty rather than covered.
        assert_eq!(
            coverage(
                "alpha",
                &loaded,
                &none,
                &paths(&["alpha/a.yaml", "alpha/b.yaml", "alpha/sub/c.yaml"]),
                &none,
            ),
            Coverage::Empty
        );
    }

    #[test]
    fn subtree_loaded_needs_every_directory_underneath() {
        assert!(subtree_loaded("alpha", &loaded_tree()));
        assert!(!subtree_loaded("alpha/unopened", &loaded_tree()));

        let mut partial = loaded_tree();
        partial.get_mut("alpha").unwrap().push(dir("deep"));
        assert!(!subtree_loaded("alpha", &partial));
    }

    #[test]
    fn directories_are_told_apart_from_contracts_by_the_tree() {
        let loaded = loaded_tree();

        assert!(is_directory_path("alpha", &roots(), &loaded));
        assert!(is_directory_path("alpha/sub", &roots(), &loaded));
        assert!(!is_directory_path("alpha/a.yaml", &roots(), &loaded));
        assert!(!is_directory_path("root.yaml", &roots(), &loaded));
    }

    #[test]
    fn known_contracts_are_collected_from_every_loaded_level() {
        let mut known = Vec::new();
        collect_known_contracts(&roots(), None, &loaded_tree(), &mut known);

        assert_eq!(
            known,
            vec![
                "alpha/a.yaml".to_string(),
                "alpha/sub/c.yaml".to_string(),
                "alpha/b.yaml".to_string(),
                "root.yaml".to_string(),
            ]
        );
    }

    #[test]
    fn addable_paths_drop_duplicates_and_anything_already_in_hand() {
        let addable = filter_addable(
            vec![
                "a.yaml".to_string(),
                "a.yaml".to_string(),
                "b.yaml".to_string(),
                "c.yaml".to_string(),
            ],
            &paths(&["b.yaml"]),
            &paths(&["c.yaml"]),
        );

        assert_eq!(addable, vec!["a.yaml".to_string()]);
    }
}
