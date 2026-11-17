//! Watching followed contract files and directories, for `GET /events`.
//!
//! Each `/events` connection subscribes with a [`FollowSet`]. The
//! [`WatchManager`] turns that into OS watches - the parent directory
//! (non-recursive) of a followed file, the directory itself (recursive) for a
//! followed directory - shared and reference-counted across subscribers. One
//! debounced watcher feeds a dispatch task that keeps the storage path map
//! current and hands each subscriber a [`ChangeBatch`] filtered to its set.

use futures_util::Stream;
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::warn;

use crate::storage::{self, Storage};

/// How long the debouncer waits for a burst of events (an editor's
/// temp-file-and-rename save, a `git checkout`) to settle.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// What one client follows: single contract files and whole directories, as
/// `/`-separated paths relative to the base directory. The empty directory
/// path is the base directory itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FollowSet {
    pub files: BTreeSet<String>,
    pub dirs: BTreeSet<String>,
}

impl FollowSet {
    /// Parse `file=<path>&dir=<path>` from a raw query string. Both keys may
    /// repeat; unknown keys are an error so a typo is not silently ignored.
    pub fn from_query(query: &str) -> Result<Self, String> {
        let mut follow = FollowSet::default();
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            match key.as_ref() {
                "file" => {
                    validate_follow_path(&value)?;
                    if !storage::is_contract_file(Path::new(value.as_ref())) {
                        return Err(format!("Not a contract file (.yaml/.yml): '{}'", value));
                    }
                    follow.files.insert(value.into_owned());
                }
                "dir" => {
                    let dir = value.trim_end_matches('/');
                    if !dir.is_empty() {
                        validate_follow_path(dir)?;
                    }
                    follow.dirs.insert(dir.to_string());
                }
                other => return Err(format!("Unknown parameter: '{}'", other)),
            }
        }
        Ok(follow)
    }

    fn under_dir(&self, path: &str) -> bool {
        self.dirs.iter().any(|dir| {
            dir.is_empty()
                || path
                    .strip_prefix(dir.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    fn contains(&self, path: &str) -> bool {
        self.files.contains(path) || self.under_dir(path)
    }
}

/// Same rules as `PUT`, plus no empty segments, since the follow set is
/// matched against path-map keys as plain strings.
fn validate_follow_path(path: &str) -> Result<(), String> {
    if path.split('/').any(str::is_empty) {
        return Err(format!("Empty path segment in '{}'", path));
    }
    storage::validate_contract_path(path).map_err(|e| e.to_string())
}

/// One debounced batch of changes, as sent in an SSE `data:` line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ChangeBatch {
    pub added: BTreeSet<String>,
    pub changed: BTreeSet<String>,
    pub removed: BTreeSet<String>,
}

impl ChangeBatch {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// What the watcher saw happen to one contract path in a batch.
#[derive(Debug, Clone, Copy)]
struct Observed {
    exists: bool,
    /// Whether the path map had it before this batch.
    was_known: bool,
}

/// One watched directory. It may be reached under several relative names
/// (a symlinked directory and its target), each with its own counts.
struct Root {
    names: HashMap<String, Counts>,
    /// The mode the OS watch is currently registered with.
    mode: RecursiveMode,
}

#[derive(Default)]
struct Counts {
    recursive: usize,
    flat: usize,
}

impl Root {
    fn wanted_mode(&self) -> RecursiveMode {
        if self.names.values().any(|counts| counts.recursive > 0) {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }
}

/// A watch one subscriber holds: canonical directory, relative name, recursive.
type Hold = (PathBuf, String, bool);

struct Subscriber {
    follow: FollowSet,
    holds: Vec<Hold>,
    tx: mpsc::UnboundedSender<ChangeBatch>,
}

struct Inner {
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    /// Canonical directory -> its watch.
    roots: HashMap<PathBuf, Root>,
    subscribers: HashMap<u64, Subscriber>,
    next_id: u64,
}

struct Shared {
    storage: Arc<RwLock<Storage>>,
    base_dir: PathBuf,
    inner: Mutex<Inner>,
}

/// Shared, reference-counted file watching for every `/events` client.
#[derive(Clone)]
pub struct WatchManager {
    shared: Arc<Shared>,
}

impl WatchManager {
    /// Start the (idle) watcher and its dispatch task. Must be called inside
    /// a Tokio runtime.
    pub fn new(storage: Arc<RwLock<Storage>>, base_dir: PathBuf) -> anyhow::Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<PathBuf>>();

        let debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    let paths: Vec<PathBuf> = events
                        .into_iter()
                        // Reads (inotify open/close) say nothing about content.
                        .filter(|event| !matches!(event.kind, EventKind::Access(_)))
                        .flat_map(|event| event.event.paths)
                        .collect();
                    if !paths.is_empty() {
                        let _ = tx.send(paths);
                    }
                }
                Err(errors) => {
                    for error in errors {
                        warn!("File watch error: {}", error);
                    }
                }
            }
        })?;

        let shared = Arc::new(Shared {
            storage,
            base_dir,
            inner: Mutex::new(Inner {
                debouncer,
                roots: HashMap::new(),
                subscribers: HashMap::new(),
                next_id: 0,
            }),
        });

        // The task holds only a weak reference; dropping the last manager
        // drops the debouncer, which closes the channel and ends the task.
        let weak = Arc::downgrade(&shared);
        tokio::spawn(async move {
            while let Some(paths) = rx.recv().await {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                shared.dispatch(paths).await;
            }
        });

        Ok(Self { shared })
    }

    /// Start watching everything in `follow`. Watches are in place when this
    /// returns; they are released when the returned stream is dropped.
    ///
    /// A followed file or directory whose directory does not exist (or leaves
    /// the base directory) is not watched; the rest of the set still is.
    ///
    /// The path map only tracks the disk while a watch covers it, so once the
    /// watches are in place the followed paths are reconciled with the disk:
    /// contracts that appeared or vanished while unwatched are picked up or
    /// dropped, and later events report them against an accurate map.
    pub async fn subscribe(&self, follow: FollowSet) -> Subscription {
        let subscription = self.watch(follow.clone());
        self.shared.reconcile(&follow).await;
        subscription
    }

    fn watch(&self, follow: FollowSet) -> Subscription {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inner = self.shared.lock();

        let wanted = follow
            .files
            .iter()
            .map(|file| (file.rsplit_once('/').map_or("", |(dir, _)| dir), false))
            .chain(follow.dirs.iter().map(|dir| (dir.as_str(), true)));

        let mut holds = Vec::new();
        for (name, recursive) in wanted {
            if let Some(canonical) = self.shared.acquire(&mut inner, name, recursive) {
                holds.push((canonical, name.to_string(), recursive));
            }
        }

        let id = inner.next_id;
        inner.next_id += 1;
        inner.subscribers.insert(id, Subscriber { follow, holds, tx });

        Subscription {
            id,
            shared: self.shared.clone(),
            rx,
        }
    }

    /// Currently watched directories and whether each is recursive.
    #[cfg(test)]
    fn watched(&self) -> Vec<(PathBuf, bool)> {
        let inner = self.shared.lock();
        let mut watched: Vec<_> = inner
            .roots
            .iter()
            .map(|(path, root)| (path.clone(), root.mode == RecursiveMode::Recursive))
            .collect();
        watched.sort();
        watched
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Bring the path map in line with the disk for everything `follow`
    /// names. Nothing is reported; the subscriber fetches what it follows.
    async fn reconcile(&self, follow: &FollowSet) {
        let mut storage = self.storage.write().await;
        for dir in &follow.dirs {
            storage.forget_missing_under(dir);
            storage.scan_new_under(dir);
        }
        for file in &follow.files {
            if storage.contract_exists(file) {
                storage.adopt_contract(file);
            } else {
                storage.forget_contract(file);
            }
        }
    }

    /// Take one reference on the watch for directory `name` (relative).
    fn acquire(&self, inner: &mut Inner, name: &str, recursive: bool) -> Option<PathBuf> {
        let path = self.base_dir.join(name);
        let Some(canonical) = storage::resolve_within(&path, &self.base_dir) else {
            warn!("Not watching {:?}: missing or outside the base directory", path);
            return None;
        };
        if !canonical.is_dir() {
            warn!("Not watching {:?}: not a directory", path);
            return None;
        }

        let Inner {
            debouncer, roots, ..
        } = inner;
        let is_new = !roots.contains_key(&canonical);
        let root = roots.entry(canonical.clone()).or_insert_with(|| Root {
            names: HashMap::new(),
            mode: RecursiveMode::NonRecursive,
        });
        let counts = root.names.entry(name.to_string()).or_default();
        if recursive {
            counts.recursive += 1;
        } else {
            counts.flat += 1;
        }

        let wanted = root.wanted_mode();
        if is_new {
            if let Err(e) = debouncer.watch(&canonical, wanted) {
                warn!("Failed to watch {:?}: {}", canonical, e);
                roots.remove(&canonical);
                return None;
            }
            root.mode = wanted;
        } else if wanted != root.mode {
            // Upgrading a shared watch to recursive. If that fails, give back
            // only this reference and put the existing watch back, so other
            // subscribers keep what they hold.
            let _ = unwatch(debouncer, &canonical);
            let root = roots.get_mut(&canonical).expect("root just used");
            match debouncer.watch(&canonical, wanted) {
                Ok(()) => root.mode = wanted,
                Err(e) => {
                    warn!("Failed to watch {:?} recursively: {}", canonical, e);
                    let counts = root.names.get_mut(name).expect("counts just taken");
                    if recursive {
                        counts.recursive -= 1;
                    } else {
                        counts.flat -= 1;
                    }
                    if counts.recursive == 0 && counts.flat == 0 {
                        root.names.remove(name);
                    }
                    let previous = root.mode;
                    if let Err(e) = debouncer.watch(&canonical, previous) {
                        warn!("Failed to restore the watch on {:?}: {}", canonical, e);
                        roots.remove(&canonical);
                    }
                    return None;
                }
            }
        }

        Some(canonical)
    }

    /// Give back one reference taken by `acquire`.
    fn release(&self, inner: &mut Inner, (canonical, name, recursive): &Hold) {
        let Inner {
            debouncer, roots, ..
        } = inner;
        let Some(root) = roots.get_mut(canonical) else {
            return;
        };
        if let Some(counts) = root.names.get_mut(name) {
            if *recursive {
                counts.recursive = counts.recursive.saturating_sub(1);
            } else {
                counts.flat = counts.flat.saturating_sub(1);
            }
            if counts.recursive == 0 && counts.flat == 0 {
                root.names.remove(name);
            }
        }

        if root.names.is_empty() {
            roots.remove(canonical);
            let _ = unwatch(debouncer, canonical);
            return;
        }

        let wanted = root.wanted_mode();
        if wanted != root.mode {
            let previous = root.mode;
            let _ = unwatch(debouncer, canonical);
            let root = roots.get_mut(canonical).expect("root just used");
            match debouncer.watch(canonical, wanted) {
                Ok(()) => root.mode = wanted,
                Err(e) => {
                    // A recursive watch still covers what a flat one would;
                    // dispatch filters by depth, so keep the old mode.
                    warn!("Failed to re-watch {:?} non-recursively: {}", canonical, e);
                    if let Err(e) = debouncer.watch(canonical, previous) {
                        warn!("Failed to restore the watch on {:?}: {}", canonical, e);
                        roots.remove(canonical);
                    }
                }
            }
        }
    }

    /// Route one debounced batch of absolute paths to subscribers.
    async fn dispatch(&self, paths: Vec<PathBuf>) {
        // Snapshot routing state; the lock is not held across the await below.
        let (candidates, subscribers) = {
            let inner = self.lock();
            let mut candidates: BTreeMap<String, bool> = BTreeMap::new();
            for path in &paths {
                for (canonical, root) in &inner.roots {
                    let Ok(rest) = path.strip_prefix(canonical) else {
                        continue;
                    };
                    let Some(rest) = relative_str(rest) else {
                        continue;
                    };
                    let depth = rest.split('/').filter(|s| !s.is_empty()).count();
                    // An event on a watched directory itself matters only when
                    // the directory is gone. Removing a tree can arrive as just
                    // that one event (macOS collapses the children into it), so
                    // sweep everything under it, whatever the watch's mode:
                    // a followed file's parent going takes the file with it.
                    let root_gone = depth == 0 && !canonical.exists();
                    for (name, counts) in &root.names {
                        if root_gone {
                            *candidates.entry(name.clone()).or_default() = true;
                            continue;
                        }
                        let recursive = counts.recursive > 0;
                        if depth == 0 || (!recursive && depth != 1) {
                            continue;
                        }
                        let relative = if name.is_empty() {
                            rest.clone()
                        } else {
                            format!("{}/{}", name, rest)
                        };
                        *candidates.entry(relative).or_default() |= recursive;
                    }
                }
            }
            let subscribers: Vec<_> = inner
                .subscribers
                .values()
                .map(|sub| (sub.follow.clone(), sub.tx.clone()))
                .collect();
            (candidates, subscribers)
        };

        if candidates.is_empty() {
            return;
        }

        let observed = {
            let mut storage = self.storage.write().await;
            let mut observed: BTreeMap<String, Observed> = BTreeMap::new();
            for (relative, recursive) in &candidates {
                for (path, seen) in observe(&mut storage, relative, *recursive) {
                    // A path seen twice in one batch (via its directory and
                    // itself) keeps the first "known before" and the last state.
                    observed
                        .entry(path)
                        .and_modify(|first| first.exists = seen.exists)
                        .or_insert(seen);
                }
            }
            observed
        };

        for (follow, tx) in subscribers {
            let batch = classify(&follow, &observed);
            if !batch.is_empty() {
                let _ = tx.send(batch);
            }
        }
    }
}

/// Drop the OS watch on `canonical`.
///
/// Known gap: `Debouncer::unwatch` also forgets the file-ID cache roots of
/// every watched directory *below* `canonical` (it matches by prefix), so a
/// nested watch loses rename stitching and rescans until it is re-registered.
/// The debouncer cannot re-add a cache root without registering the OS watch
/// a second time (duplicate FSEvents paths, shared inotify descriptors), so
/// this is left alone: events from the nested directory still arrive, and
/// dispatch re-reads the disk for every path it is told about.
fn unwatch(
    debouncer: &mut Debouncer<RecommendedWatcher, RecommendedCache>,
    canonical: &Path,
) -> notify::Result<()> {
    debouncer.unwatch(canonical)
}

/// `/`-separated form of a relative path, or `None` for non-UTF-8 names.
fn relative_str(path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path.components().map(|c| c.as_os_str().to_str()).collect();
    parts.map(|parts| parts.join("/"))
}

/// Bring the path map in line with what is on disk at `relative`, and report
/// the contract paths affected. A directory under a recursive watch is
/// rescanned when present (new contracts) or swept from the map when gone.
fn observe(storage: &mut Storage, relative: &str, recursive: bool) -> Vec<(String, Observed)> {
    if storage::has_hidden_segment(relative) {
        return Vec::new();
    }

    if storage::is_contract_file(Path::new(relative)) {
        let was_known = storage.knows_contract(relative);
        let exists = storage.contract_exists(relative);
        if exists && !was_known {
            storage.register_contract(relative);
        } else if !exists && was_known {
            storage.forget_contract(relative);
        }
        if !exists && !was_known {
            return Vec::new();
        }
        return vec![(relative.to_string(), Observed { exists, was_known })];
    }

    if !recursive {
        return Vec::new();
    }

    if storage.base_dir().join(relative).is_dir() {
        storage
            .scan_new_under(relative)
            .into_iter()
            .map(|path| (path, Observed { exists: true, was_known: false }))
            .collect()
    } else {
        storage
            .forget_contracts_under(relative)
            .into_iter()
            .map(|path| (path, Observed { exists: false, was_known: true }))
            .collect()
    }
}

/// The part of a batch one subscriber follows.
fn classify(follow: &FollowSet, observed: &BTreeMap<String, Observed>) -> ChangeBatch {
    let mut batch = ChangeBatch::default();
    for (path, seen) in observed {
        if !follow.contains(path) {
            continue;
        }
        match (seen.exists, seen.was_known) {
            (true, true) => {
                batch.changed.insert(path.clone());
            }
            // A directly followed file coming back is a change to it; only a
            // followed directory gains new files.
            (true, false) if follow.files.contains(path) => {
                batch.changed.insert(path.clone());
            }
            (true, false) => {
                batch.added.insert(path.clone());
            }
            (false, true) => {
                batch.removed.insert(path.clone());
            }
            (false, false) => {}
        }
    }
    batch
}

/// The stream of batches for one `/events` connection. Dropping it (the
/// client disconnecting) releases its watches.
pub struct Subscription {
    id: u64,
    shared: Arc<Shared>,
    rx: mpsc::UnboundedReceiver<ChangeBatch>,
}

impl Stream for Subscription {
    type Item = ChangeBatch;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut inner = self.shared.lock();
        if let Some(subscriber) = inner.subscribers.remove(&self.id) {
            for hold in &subscriber.holds {
                self.shared.release(&mut inner, hold);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::time::Instant;

    /// Generous: FSEvents on macOS can take a while to deliver.
    const WAIT: Duration = Duration::from_secs(10);
    /// How long to listen when asserting nothing (more) arrives.
    const QUIET: Duration = Duration::from_millis(1500);

    struct Fixture {
        dir: tempfile::TempDir,
        storage: Arc<RwLock<Storage>>,
        manager: WatchManager,
    }

    impl Fixture {
        fn new(files: &[&str]) -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            for file in files {
                write(&dir.path().join(file), "kind: Agent\nname: a\n");
            }
            let storage = Storage::new(dir.path()).expect("storage");
            let base_dir = storage.base_dir().to_path_buf();
            let storage = Arc::new(RwLock::new(storage));
            let manager = WatchManager::new(storage.clone(), base_dir).expect("manager");
            Fixture {
                dir,
                storage,
                manager,
            }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.dir.path().join(relative)
        }

        async fn subscribe(&self, query: &str) -> Subscription {
            let sub = self
                .manager
                .subscribe(FollowSet::from_query(query).expect("query"))
                .await;
            // Let the native watcher settle so setup writes are not reported
            // and the first test write is.
            tokio::time::sleep(Duration::from_millis(300)).await;
            sub
        }
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, content).expect("write");
    }

    /// Merge batches until `done` holds for the union, or fail after WAIT.
    async fn until(sub: &mut Subscription, done: impl Fn(&ChangeBatch) -> bool) -> ChangeBatch {
        let deadline = Instant::now() + WAIT;
        let mut all = ChangeBatch::default();
        while !done(&all) {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, sub.next()).await {
                Ok(Some(batch)) => {
                    all.added.extend(batch.added);
                    all.changed.extend(batch.changed);
                    all.removed.extend(batch.removed);
                }
                _ => panic!("timed out waiting; got so far: {:?}", all),
            }
        }
        all
    }

    /// Everything that arrives within QUIET.
    async fn drain(sub: &mut Subscription) -> ChangeBatch {
        let mut all = ChangeBatch::default();
        while let Ok(Some(batch)) = tokio::time::timeout(QUIET, sub.next()).await {
            all.added.extend(batch.added);
            all.changed.extend(batch.changed);
            all.removed.extend(batch.removed);
        }
        all
    }

    fn set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn follow_query_parses_repeats_and_rejects_bad_paths() {
        let follow = FollowSet::from_query("file=a.yaml&file=sub%2Fb.yml&dir=sub/&dir=x").unwrap();
        assert_eq!(follow.files, set(&["a.yaml", "sub/b.yml"]));
        assert_eq!(follow.dirs, set(&["sub", "x"]));

        assert_eq!(FollowSet::from_query("dir=").unwrap().dirs, set(&[""]));
        assert_eq!(FollowSet::from_query("").unwrap(), FollowSet::default());

        for bad in [
            "file=../x.yaml",
            "file=a/../x.yaml",
            "file=./x.yaml",
            "file=.hidden.yaml",
            "file=.git/x.yaml",
            "file=/etc/x.yaml",
            "file=a//x.yaml",
            "file=notes.txt",
            "dir=..",
            "dir=.git",
            "dir=/abs",
            "other=x",
        ] {
            assert!(FollowSet::from_query(bad).is_err(), "accepted {}", bad);
        }
    }

    #[tokio::test]
    async fn edit_and_atomic_rename_save_are_changes() {
        let fx = Fixture::new(&["kube.yaml"]);
        let mut sub = fx.subscribe("file=kube.yaml").await;

        write(&fx.path("kube.yaml"), "kind: Agent\nname: b\n");
        let got = until(&mut sub, |b| b.changed.contains("kube.yaml")).await;
        assert!(got.added.is_empty() && got.removed.is_empty(), "{:?}", got);
        drain(&mut sub).await;

        // Editor-style save: write a temp file, rename it over the original.
        write(&fx.path("kube.yaml.tmp"), "kind: Agent\nname: c\n");
        std::fs::rename(fx.path("kube.yaml.tmp"), fx.path("kube.yaml")).unwrap();
        let got = until(&mut sub, |b| b.changed.contains("kube.yaml")).await;
        let rest = drain(&mut sub).await;
        assert_eq!(got.changed, set(&["kube.yaml"]));
        assert!(got.added.is_empty() && got.removed.is_empty(), "{:?}", got);
        assert!(rest.removed.is_empty() && rest.added.is_empty(), "{:?}", rest);
    }

    #[tokio::test]
    async fn followed_dir_reports_adds_changes_and_removes() {
        let fx = Fixture::new(&["sub/one.yaml", "other.yaml"]);
        let mut sub = fx.subscribe("dir=sub").await;

        write(&fx.path("sub/two.yaml"), "kind: Agent\nname: two\n");
        write(&fx.path("sub/deep/three.yml"), "kind: Agent\nname: three\n");
        let got = until(&mut sub, |b| b.added.len() == 2).await;
        assert_eq!(got.added, set(&["sub/deep/three.yml", "sub/two.yaml"]));

        // New files are in the path map, so GET would serve them.
        {
            let storage = fx.storage.read().await;
            assert!(storage.load_contract("sub/two.yaml").is_ok());
            assert!(storage.load_contract("sub/deep/three.yml").is_ok());
        }
        drain(&mut sub).await;

        write(&fx.path("sub/one.yaml"), "kind: Agent\nname: one-b\n");
        until(&mut sub, |b| b.changed.contains("sub/one.yaml")).await;
        drain(&mut sub).await;

        std::fs::remove_file(fx.path("sub/one.yaml")).unwrap();
        let got = until(&mut sub, |b| b.removed.contains("sub/one.yaml")).await;
        assert!(got.added.is_empty(), "{:?}", got);
        assert!(fx.storage.read().await.load_contract("sub/one.yaml").is_err());
        drain(&mut sub).await;

        // Removing a whole directory removes everything under it.
        std::fs::remove_dir_all(fx.path("sub/deep")).unwrap();
        until(&mut sub, |b| b.removed.contains("sub/deep/three.yml")).await;

        // Hidden and non-contract files never show up.
        write(&fx.path("sub/.swap.yaml"), "x");
        write(&fx.path("sub/notes.txt"), "x");
        write(&fx.path("other.yaml"), "kind: Agent\nname: other-b\n");
        assert_eq!(drain(&mut sub).await, ChangeBatch::default());
    }

    #[tokio::test]
    async fn directly_followed_file_removed_then_recreated() {
        let fx = Fixture::new(&["kube.yaml"]);
        let mut sub = fx.subscribe("file=kube.yaml").await;

        std::fs::remove_file(fx.path("kube.yaml")).unwrap();
        until(&mut sub, |b| b.removed.contains("kube.yaml")).await;
        drain(&mut sub).await;

        write(&fx.path("kube.yaml"), "kind: Agent\nname: back\n");
        let got = until(&mut sub, |b| b.changed.contains("kube.yaml")).await;
        assert!(got.added.is_empty(), "{:?}", got);
        assert!(fx.storage.read().await.load_contract("kube.yaml").is_ok());
    }

    #[tokio::test]
    async fn events_are_filtered_to_the_follow_set() {
        let fx = Fixture::new(&["followed.yaml", "sibling.yaml"]);
        let mut sub = fx.subscribe("file=followed.yaml").await;
        let mut sibling_sub = fx.subscribe("file=sibling.yaml").await;

        write(&fx.path("sibling.yaml"), "kind: Agent\nname: s\n");
        write(&fx.path("brand-new.yaml"), "kind: Agent\nname: n\n");
        until(&mut sibling_sub, |b| b.changed.contains("sibling.yaml")).await;

        // Positive control: the followed file still comes through.
        write(&fx.path("followed.yaml"), "kind: Agent\nname: f\n");
        let got = until(&mut sub, |b| b.changed.contains("followed.yaml")).await;
        let rest = drain(&mut sub).await;
        assert_eq!(got.changed, set(&["followed.yaml"]));
        assert!(got.added.is_empty() && got.removed.is_empty(), "{:?}", got);
        assert_eq!(rest, ChangeBatch::default());
    }

    #[tokio::test]
    async fn watches_are_shared_and_released_with_the_last_subscriber() {
        let fx = Fixture::new(&["a.yaml", "b.yaml", "sub/c.yaml"]);
        let base = fx.storage.read().await.base_dir().to_path_buf();
        assert!(fx.manager.watched().is_empty());

        let first = fx.subscribe("file=a.yaml&file=b.yaml").await;
        let second = fx.subscribe("file=a.yaml").await;
        assert_eq!(fx.manager.watched(), vec![(base.clone(), false)]);

        // Following the same directory recursively upgrades the one watch.
        let third = fx.subscribe("dir=").await;
        assert_eq!(fx.manager.watched(), vec![(base.clone(), true)]);
        drop(third);
        assert_eq!(fx.manager.watched(), vec![(base.clone(), false)]);

        let fourth = fx.subscribe("file=sub/c.yaml&file=missing/x.yaml").await;
        assert_eq!(
            fx.manager.watched(),
            vec![(base.clone(), false), (base.join("sub"), false)]
        );
        drop(fourth);

        drop(first);
        assert_eq!(fx.manager.watched(), vec![(base.clone(), false)]);
        drop(second);
        assert!(fx.manager.watched().is_empty());
        assert!(fx.manager.shared.lock().subscribers.is_empty());
    }

    #[tokio::test]
    async fn removing_a_followed_directory_removes_its_contracts() {
        let fx = Fixture::new(&["sub/one.yaml", "sub/deep/two.yaml", "other.yaml"]);
        let mut sub = fx.subscribe("dir=sub").await;

        std::fs::remove_dir_all(fx.path("sub")).unwrap();
        let got = until(&mut sub, |b| b.removed.len() == 2).await;
        assert_eq!(got.removed, set(&["sub/deep/two.yaml", "sub/one.yaml"]));
        assert!(got.added.is_empty() && got.changed.is_empty(), "{:?}", got);
        assert_eq!(
            fx.storage.read().await.list_contracts(),
            vec!["other.yaml".to_string()]
        );
    }

    #[tokio::test]
    async fn removing_a_followed_files_directory_removes_the_file() {
        let fx = Fixture::new(&["sub/one.yaml", "sub/two.yaml"]);
        let mut sub = fx.subscribe("file=sub/one.yaml").await;

        std::fs::remove_dir_all(fx.path("sub")).unwrap();
        let got = until(&mut sub, |b| b.removed.contains("sub/one.yaml")).await;
        let rest = drain(&mut sub).await;
        assert_eq!(got.removed, set(&["sub/one.yaml"]));
        assert_eq!(rest, ChangeBatch::default());
        assert!(fx.storage.read().await.list_contracts().is_empty());
    }

    #[tokio::test]
    async fn subscribing_picks_up_what_changed_while_unwatched() {
        let fx = Fixture::new(&["sub/old.yaml", "gone.yaml"]);

        // Nobody watches yet, so the path map does not see these.
        write(&fx.path("sub/new.yaml"), "kind: Agent\nname: new\n");
        write(&fx.path("lone.yaml"), "kind: Agent\nname: lone\n");
        std::fs::remove_file(fx.path("sub/old.yaml")).unwrap();
        std::fs::remove_file(fx.path("gone.yaml")).unwrap();
        assert!(fx.storage.read().await.load_contract("sub/new.yaml").is_err());

        let mut sub = fx.subscribe("dir=sub&file=lone.yaml&file=gone.yaml").await;
        assert_eq!(
            fx.storage.read().await.list_contracts(),
            vec!["lone.yaml".to_string(), "sub/new.yaml".to_string()]
        );

        // Their later removal is reported, since the map now knows them.
        std::fs::remove_file(fx.path("sub/new.yaml")).unwrap();
        std::fs::remove_file(fx.path("lone.yaml")).unwrap();
        let got = until(&mut sub, |b| b.removed.len() == 2).await;
        assert_eq!(got.removed, set(&["lone.yaml", "sub/new.yaml"]));
    }
}
