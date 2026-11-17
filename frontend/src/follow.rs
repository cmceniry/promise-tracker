//! Following server contract files: the pure half.
//!
//! Everything here decides *what* should happen when the server's copy of a
//! followed contract moves — which paths the event stream should name, which
//! cards an event touches, how a fetched text lands on a card, what a card's
//! status reads as. None of it touches the browser, so it is unit tested on
//! the host with `cargo test -p promise_tracker_frontend`. The browser half
//! (EventSource, fetches, signals) lives in `follow_sync.rs` and calls in here.
//!
//! Directory following: a followed directory controls every contract file
//! under it (recursively) unless that path is in the global exclusion list.
//! Directory keys are `/`-trimmed server paths; the empty string is the root
//! of the server's base dir.

use serde::Deserialize;

use crate::models::{Contract, FollowState, PendingServerCopy};
use crate::utils::{compare_contracts, validate_contract_content};

/// What the browser follows: the query of the one shared `/events` stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FollowSet {
    /// Directly followed files (sorted, deduplicated server paths).
    pub files: Vec<String>,
    /// Followed directories (sorted, deduplicated server paths).
    pub dirs: Vec<String>,
}

impl FollowSet {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.dirs.is_empty()
    }

    /// The same-origin URL of the event stream for this set, or None when
    /// nothing is followed (and no stream should be open).
    pub fn events_url(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let params: Vec<String> = self
            .files
            .iter()
            .map(|f| format!("file={}", encode_query_value(f)))
            .chain(
                self.dirs
                    .iter()
                    .map(|d| format!("dir={}", encode_query_value(d))),
            )
            .collect();
        Some(format!("/events?{}", params.join("&")))
    }
}

/// The follow set for these cards. Only `Direct` cards name their file: a
/// `ViaDir` card is covered by its directory, which the caller passes in.
pub fn follow_set(contracts: &[Contract], followed_dirs: &[String]) -> FollowSet {
    let mut files: Vec<String> = contracts
        .iter()
        .filter(|c| c.follow == FollowState::Direct)
        .filter_map(|c| c.server_path.clone())
        .filter(|p| !p.trim().is_empty())
        .collect();
    files.sort();
    files.dedup();

    // The empty directory is the server root, and is a real follow.
    let mut dirs: Vec<String> = followed_dirs.iter().map(|d| normalize_dir(d)).collect();
    dirs.sort();
    dirs.dedup();

    FollowSet { files, dirs }
}

/// Every server path some followed card shows: what to refetch whenever the
/// stream (re)connects, since the stream never replays what it missed.
pub fn followed_paths(contracts: &[Contract]) -> Vec<String> {
    let mut paths: Vec<String> = contracts
        .iter()
        .filter(|c| c.follow != FollowState::Off)
        .filter_map(|c| c.server_path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Percent-encode a query value (RFC 3986 unreserved characters pass).
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// One event off the stream: paths relative to the server's base dir.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ChangeEvent {
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
}

impl ChangeEvent {
    pub fn parse(data: &str) -> Result<Self, String> {
        serde_json::from_str(data).map_err(|e| e.to_string())
    }
}

/// What to do about one event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventPlan {
    /// Server paths to fetch again; each fetch lands on every card showing it.
    pub refetch: Vec<String>,
    /// Server paths whose directly followed cards are now missing.
    pub missing: Vec<String>,
    /// New files under a followed directory, with no card yet: fetch each and
    /// append a `ViaDir` card for it.
    pub create: Vec<String>,
    /// Paths of `ViaDir` cards the server says are gone. Each is refetched
    /// first, and its cards are removed only if the server confirms a 404, so
    /// a remove racing a re-create never drops a live card.
    pub vanished: Vec<String>,
}

/// Plan the response to an event. Paths nobody follows are ignored, so a
/// stale event racing a follow-set change does no harm.
///
/// A file saved through `PUT` reaches other browsers as `changed`, not
/// `added`, so a `changed` path with no card under a followed directory is
/// treated as new too.
pub fn plan_event(
    contracts: &[Contract],
    event: &ChangeEvent,
    followed_dirs: &[String],
    exclusions: &[String],
) -> EventPlan {
    let followed = |path: &String| {
        contracts
            .iter()
            .any(|c| c.follow != FollowState::Off && c.server_path.as_ref() == Some(path))
    };
    let with_follow = |path: &String, state: FollowState| {
        contracts
            .iter()
            .any(|c| c.follow == state && c.server_path.as_ref() == Some(path))
    };
    let has_card = |path: &String| contracts.iter().any(|c| c.server_path.as_ref() == Some(path));

    let mut refetch: Vec<String> = event
        .added
        .iter()
        .chain(event.changed.iter())
        .filter(|p| followed(p))
        .cloned()
        .collect();
    sort_dedup(&mut refetch);

    let mut create: Vec<String> = event
        .added
        .iter()
        .chain(event.changed.iter())
        .filter(|p| !has_card(p) && dir_controls(p, followed_dirs, exclusions))
        .cloned()
        .collect();
    sort_dedup(&mut create);

    let mut missing: Vec<String> = event
        .removed
        .iter()
        .filter(|p| with_follow(p, FollowState::Direct))
        .cloned()
        .collect();
    sort_dedup(&mut missing);

    let mut vanished: Vec<String> = event
        .removed
        .iter()
        .filter(|p| with_follow(p, FollowState::ViaDir))
        .cloned()
        .collect();
    sort_dedup(&mut vanished);

    EventPlan {
        refetch,
        missing,
        create,
        vanished,
    }
}

fn sort_dedup(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

/// How a fetched server text should land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyMode {
    /// Starting to follow: the server copy replaces whatever the card held,
    /// so a local copy is never mistaken for a "last good" server version.
    Adopt,
    /// Already following: keep the last good version if the new text is bad.
    Follow,
}

/// What applying a server text did to the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// Same text as the card already showed (pending cleared if any).
    Unchanged,
    /// Valid new text; the card now shows it.
    Updated,
    /// Invalid text; the card keeps its last good content, the text is pending.
    KeptLastGood,
    /// Invalid text and no good version to fall back on: the card is in error
    /// and stays out of the graph.
    NoGoodVersion,
}

impl Applied {
    /// Whether the card visibly changed (worth a flash).
    pub fn landed(self) -> bool {
        !matches!(self, Applied::Unchanged)
    }
}

/// Land a fetched server text on a followed card.
///
/// A card has a good version when its `content` validates (`err` empty).
/// Valid text replaces the content and clears any pending bad copy. Invalid
/// text, in `Follow` mode on a card with a good version, is parked in
/// `pending` and the content left alone; otherwise the bad text becomes the
/// content with its error, so the card shows the error and the graph skips it.
pub fn apply_server_text(contract: &mut Contract, text: &str, mode: ApplyMode) -> Applied {
    let err = validate_contract_content(text);
    let has_good = contract.err.is_empty();

    if err.is_empty() {
        let same = contract.content == text && contract.pending.is_none() && has_good;
        contract.content = text.to_string();
        contract.err = String::new();
        contract.pending = None;
        return if same {
            Applied::Unchanged
        } else {
            Applied::Updated
        };
    }

    if mode == ApplyMode::Follow && has_good {
        let pending = PendingServerCopy {
            text: text.to_string(),
            err,
        };
        if contract.pending.as_ref() == Some(&pending) {
            return Applied::Unchanged;
        }
        contract.pending = Some(pending);
        return Applied::KeptLastGood;
    }

    let same = contract.content == text && contract.err == err && contract.pending.is_none();
    contract.content = text.to_string();
    contract.err = err;
    contract.pending = None;
    if same {
        Applied::Unchanged
    } else {
        Applied::NoGoodVersion
    }
}

/// Whether turning follow on would throw away local edits, so needs a confirm.
pub fn needs_discard_confirm(contract: &Contract, server_text: &str) -> bool {
    compare_contracts(&contract.content, server_text)
}

/// Turn following on (`Direct` or `ViaDir`), with the server's text already
/// in hand.
pub fn begin_follow(contract: &mut Contract, server_text: &str, state: FollowState) -> Applied {
    if let Some(path) = contract.server_path.clone() {
        // The filename is locked while following; it is the server's.
        contract.filename = path;
    }
    contract.follow = state;
    contract.pending = None;
    apply_server_text(contract, server_text, ApplyMode::Adopt)
}

/// Stop following: the card keeps its content and `server_path` and becomes
/// an ordinary, editable card again.
pub fn end_follow(contract: &mut Contract) {
    contract.follow = FollowState::Off;
    contract.pending = None;
}

// ---------------------------------------------------------------------------
// Directory following
// ---------------------------------------------------------------------------

/// A directory as a follow key: no leading or trailing `/`; "" is the root.
pub fn normalize_dir(dir: &str) -> String {
    dir.trim_matches('/').to_string()
}

/// Whether `path` sits somewhere under `dir`. Segment-aware: `a` covers
/// `a/x.yaml` and `a/b/y.yaml` but not `ab/x.yaml`; the root covers all.
pub fn is_under_dir(path: &str, dir: &str) -> bool {
    let dir = dir.trim_matches('/');
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return false;
    }
    if dir.is_empty() {
        return true;
    }
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.len() > 1 && rest.starts_with('/'))
}

/// The followed directory a path is followed through: the innermost one
/// containing it, when overlapping directories both do.
pub fn controlling_dir<'a>(path: &str, followed_dirs: &'a [String]) -> Option<&'a str> {
    followed_dirs
        .iter()
        .filter(|d| is_under_dir(path, d))
        .max_by_key(|d| normalize_dir(d).len())
        .map(String::as_str)
}

/// Whether a path is dir-controlled: some followed directory contains it and
/// it is not excluded.
pub fn dir_controls(path: &str, followed_dirs: &[String], exclusions: &[String]) -> bool {
    controlling_dir(path, followed_dirs).is_some() && !exclusions.iter().any(|e| e == path)
}

/// How a directory reads on a chip or tooltip: `sub/`, or `/` for the root.
pub fn dir_label(dir: &str) -> String {
    let dir = normalize_dir(dir);
    if dir.is_empty() {
        "/".to_string()
    } else {
        format!("{}/", dir)
    }
}

/// The follow state a card gets when someone turns following on for it (from
/// its card or from the server browser): `ViaDir` inside a followed
/// directory, `Direct` otherwise. The caller lifts any exclusion first.
pub fn follow_mode_for(path: &str, followed_dirs: &[String]) -> FollowState {
    if controlling_dir(path, followed_dirs).is_some() {
        FollowState::ViaDir
    } else {
        FollowState::Direct
    }
}

/// Add a path to a sorted exclusion list (no-op if present).
pub fn add_exclusion(exclusions: &mut Vec<String>, path: &str) {
    if let Err(at) = exclusions.binary_search_by(|e| e.as_str().cmp(path)) {
        exclusions.insert(at, path.to_string());
    }
}

/// Remove a path from the exclusion list. True if it was there.
pub fn remove_exclusion(exclusions: &mut Vec<String>, path: &str) -> bool {
    let before = exclusions.len();
    exclusions.retain(|e| e != path);
    exclusions.len() != before
}

/// What the cards need when a directory's contract listing is in hand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirPlan {
    /// Listed paths with no card and no exclusion: new `ViaDir` cards, in
    /// listing (sorted) order.
    pub create: Vec<String>,
    /// `(card id, path)` of unfollowed, unexcluded cards to adopt as `ViaDir`
    /// (each with the discard confirm). `Direct` and `ViaDir` cards are left
    /// alone: adoption never downgrades `Direct`.
    pub adopt: Vec<(String, String)>,
}

/// Plan following a directory (or re-listing a followed one) given every
/// contract path under it on the server.
pub fn plan_dir_listing(contracts: &[Contract], listing: &[String], exclusions: &[String]) -> DirPlan {
    let mut paths: Vec<String> = listing.to_vec();
    sort_dedup(&mut paths);

    let mut plan = DirPlan::default();
    for path in paths {
        if exclusions.contains(&path) {
            continue;
        }
        let cards: Vec<&Contract> = contracts
            .iter()
            .filter(|c| c.server_path.as_deref() == Some(path.as_str()))
            .collect();
        if cards.is_empty() {
            plan.create.push(path);
        } else {
            plan.adopt.extend(
                cards
                    .iter()
                    .filter(|c| c.follow == FollowState::Off)
                    .map(|c| (c.id.clone(), path.clone())),
            );
        }
    }
    plan
}

/// A new card following `path`, holding the server's text, in every
/// simulation.
pub fn followed_card(
    id: String,
    path: &str,
    server_text: &str,
    simulations: &[String],
    state: FollowState,
) -> Contract {
    let mut card = Contract {
        id,
        filename: path.to_string(),
        content: String::new(),
        err: String::new(),
        sims: simulations.iter().cloned().collect(),
        server_path: Some(path.to_string()),
        follow: state,
        pending: None,
    };
    apply_server_text(&mut card, server_text, ApplyMode::Adopt);
    card
}

/// Drop the `ViaDir` cards of a path that is gone from the server. True if
/// any went.
pub fn remove_vanished(contracts: &mut Vec<Contract>, path: &str) -> bool {
    let before = contracts.len();
    contracts.retain(|c| !(c.follow == FollowState::ViaDir && c.server_path.as_deref() == Some(path)));
    contracts.len() != before
}

/// `ViaDir` cards no followed directory contains any more become ordinary
/// editable cards (`Off`), keeping content and `server_path`. `Direct` cards
/// are untouched. Returns how many were released.
pub fn release_uncovered(contracts: &mut [Contract], followed_dirs: &[String]) -> usize {
    let mut released = 0;
    for c in contracts.iter_mut().filter(|c| c.follow == FollowState::ViaDir) {
        let covered = c
            .server_path
            .as_deref()
            .is_some_and(|p| controlling_dir(p, followed_dirs).is_some());
        if !covered {
            end_follow(c);
            released += 1;
        }
    }
    released
}

/// After a directory is unfollowed: exclusions only mean something under a
/// followed directory, so drop the rest.
pub fn prune_exclusions_to_dirs(exclusions: &mut Vec<String>, followed_dirs: &[String]) {
    exclusions.retain(|e| controlling_dir(e, followed_dirs).is_some());
}

/// After a full listing of `dir`: drop exclusions under it that are no longer
/// on the server. Only call with a listing that had no errors.
pub fn prune_exclusions_listed(exclusions: &mut Vec<String>, dir: &str, listing: &[String]) {
    exclusions.retain(|e| !is_under_dir(e, dir) || listing.contains(e));
}

/// The exclusion to record when a card stops following or is deleted: its
/// path, if a followed directory would otherwise bring it straight back.
pub fn exclusion_on_release(
    contract: &Contract,
    followed_dirs: &[String],
    exclusions: &[String],
) -> Option<String> {
    let path = contract.server_path.as_deref()?;
    dir_controls(path, followed_dirs, exclusions).then(|| path.to_string())
}

/// Excluded paths that have no card: the server browser marks these, since
/// that is the only place to bring them back.
pub fn excluded_without_card(contracts: &[Contract], exclusions: &[String]) -> Vec<String> {
    exclusions
        .iter()
        .filter(|e| !contracts.iter().any(|c| c.server_path.as_deref() == Some(e.as_str())))
        .cloned()
        .collect()
}

/// Exclusions a followed directory's chip counts: those under it.
pub fn exclusions_under<'a>(exclusions: &'a [String], dir: &str) -> Vec<&'a str> {
    exclusions
        .iter()
        .filter(|e| is_under_dir(e, dir))
        .map(String::as_str)
        .collect()
}

/// Whether a card's content goes into the graph. A followed card is in when
/// it has a good version (its last good content); a broken one is left out.
pub fn in_graph(contract: &Contract) -> bool {
    contract.err.is_empty() && !contract.content.trim().is_empty()
}

/// Whether the cards as a whole hold back a graph rebuild. Only an unfollowed
/// card in error does that (as it always has); a followed card never does.
pub fn blocks_graph(contracts: &[Contract]) -> bool {
    contracts
        .iter()
        .any(|c| c.follow == FollowState::Off && !c.err.is_empty())
}

/// What a followed card knows about its server copy this session. Runtime
/// only: never persisted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchStatus {
    /// A fetch has completed (successfully or as a 404) this session.
    pub fetched: bool,
    /// The server says the file is gone.
    pub missing: bool,
    /// The last fetch failed (network, server error).
    pub fetch_error: Option<String>,
    /// Local time of the last fetch that reached the server, "HH:MM:SS".
    pub updated_at: Option<String>,
    /// Bumped each time an update visibly lands on the card.
    pub flash: u32,
}

/// The one-word status a followed card shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    /// Waiting for the first fetch.
    Syncing,
    /// Following, stream connected.
    Live,
    /// Stream disconnected or reconnecting; the card may be behind.
    Stale,
    /// The server copy is invalid (showing last good, or nothing).
    Error,
    /// The file is gone from the server.
    Missing,
}

impl CardState {
    pub fn label(self) -> &'static str {
        match self {
            CardState::Syncing => "syncing",
            CardState::Live => "live",
            CardState::Stale => "stale",
            CardState::Error => "error",
            CardState::Missing => "missing",
        }
    }
}

/// The status of a followed card. Missing outranks error (a gone file's
/// content is moot), error outranks connection state.
pub fn card_state(contract: &Contract, status: Option<&FetchStatus>, connected: bool) -> CardState {
    let status = status.cloned().unwrap_or_default();
    if status.missing {
        CardState::Missing
    } else if contract.pending.is_some() || !contract.err.is_empty() {
        CardState::Error
    } else if status.fetch_error.is_some() {
        CardState::Stale
    } else if !status.fetched {
        CardState::Syncing
    } else if !connected {
        CardState::Stale
    } else {
        CardState::Live
    }
}

/// The tooltip on a followed card's follow button and status badge.
pub fn card_tooltip(
    path: &str,
    via_dir: Option<&str>,
    state: CardState,
    status: Option<&FetchStatus>,
) -> String {
    let path = match via_dir {
        Some(dir) => format!("{} via {}", path, dir_label(dir)),
        None => path.to_string(),
    };
    let updated = status
        .and_then(|s| s.updated_at.as_deref())
        .map(|t| format!(" · updated {}", t))
        .unwrap_or_default();
    match state {
        CardState::Live => format!("following {}{}", path, updated),
        CardState::Syncing => format!("following {} · fetching", path),
        CardState::Stale => {
            let why = status
                .and_then(|s| s.fetch_error.clone())
                .unwrap_or_else(|| "reconnecting to the server".to_string());
            format!("following {} · stale: {}{}", path, why, updated)
        }
        CardState::Error => format!("following {} · server copy is invalid{}", path, updated),
        CardState::Missing => format!("following {} · missing on the server{}", path, updated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const GOOD: &str = "kind: Agent\nname: a\n";
    const GOOD2: &str = "kind: Agent\nname: b\n";
    const BAD: &str = "kind: Nonsense\nname: a\n";
    const BAD2: &str = "kind: Agent\n";

    fn card(id: &str, path: Option<&str>, follow: FollowState, content: &str) -> Contract {
        Contract {
            id: id.to_string(),
            filename: path.unwrap_or("local.yaml").to_string(),
            content: content.to_string(),
            err: validate_contract_content(content),
            sims: HashSet::new(),
            server_path: path.map(str::to_string),
            follow,
            pending: None,
        }
    }

    #[test]
    fn follow_set_names_direct_files_and_dirs() {
        let cards = vec![
            card("1", Some("b.yaml"), FollowState::Direct, GOOD),
            card("2", Some("a/x.yaml"), FollowState::Direct, GOOD),
            card("3", Some("b.yaml"), FollowState::Direct, GOOD),
            card("4", Some("off.yaml"), FollowState::Off, GOOD),
            card("5", None, FollowState::Off, GOOD),
            card("6", Some("sub/y.yaml"), FollowState::ViaDir, GOOD),
        ];
        let set = follow_set(&cards, &["sub".to_string(), "sub".to_string()]);
        assert_eq!(set.files, vec!["a/x.yaml", "b.yaml"]);
        assert_eq!(set.dirs, vec!["sub"]);
    }

    #[test]
    fn empty_follow_set_opens_no_stream() {
        let cards = vec![card("1", Some("a.yaml"), FollowState::Off, GOOD)];
        let set = follow_set(&cards, &[]);
        assert!(set.is_empty());
        assert_eq!(set.events_url(), None);
    }

    #[test]
    fn events_url_encodes_each_value() {
        let set = FollowSet {
            files: vec!["a/b c.yaml".to_string(), "x&y.yaml".to_string()],
            dirs: vec!["sub".to_string()],
        };
        assert_eq!(
            set.events_url().unwrap(),
            "/events?file=a%2Fb%20c.yaml&file=x%26y.yaml&dir=sub"
        );
    }

    #[test]
    fn followed_paths_cover_every_followed_card() {
        let cards = vec![
            card("1", Some("b.yaml"), FollowState::Direct, GOOD),
            card("2", Some("sub/y.yaml"), FollowState::ViaDir, GOOD),
            card("3", Some("off.yaml"), FollowState::Off, GOOD),
        ];
        assert_eq!(followed_paths(&cards), vec!["b.yaml", "sub/y.yaml"]);
    }

    #[test]
    fn event_refetches_followed_changes_and_marks_direct_removals_missing() {
        let cards = vec![
            card("1", Some("a.yaml"), FollowState::Direct, GOOD),
            card("2", Some("b.yaml"), FollowState::Direct, GOOD),
            card("3", Some("off.yaml"), FollowState::Off, GOOD),
        ];
        let ev = ChangeEvent::parse(
            r#"{"added":[],"changed":["a.yaml","off.yaml","nobody.yaml"],"removed":["b.yaml","off.yaml"]}"#,
        )
        .unwrap();
        let plan = plan_event(&cards, &ev, &[], &[]);
        assert_eq!(plan.refetch, vec!["a.yaml"]);
        assert_eq!(plan.missing, vec!["b.yaml"]);
    }

    #[test]
    fn event_parse_tolerates_missing_keys() {
        let ev = ChangeEvent::parse(r#"{"changed":["a.yaml"]}"#).unwrap();
        assert_eq!(ev.changed, vec!["a.yaml"]);
        assert!(ev.added.is_empty() && ev.removed.is_empty());
        assert!(ChangeEvent::parse("not json").is_err());
    }

    #[test]
    fn valid_text_updates_the_card() {
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        assert_eq!(
            apply_server_text(&mut c, GOOD2, ApplyMode::Follow),
            Applied::Updated
        );
        assert_eq!(c.content, GOOD2);
        assert!(c.err.is_empty() && c.pending.is_none());

        assert_eq!(
            apply_server_text(&mut c, GOOD2, ApplyMode::Follow),
            Applied::Unchanged
        );
    }

    #[test]
    fn invalid_text_keeps_last_good() {
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        assert_eq!(
            apply_server_text(&mut c, BAD, ApplyMode::Follow),
            Applied::KeptLastGood
        );
        assert_eq!(c.content, GOOD);
        assert!(c.err.is_empty());
        let pending = c.pending.clone().unwrap();
        assert_eq!(pending.text, BAD);
        assert!(!pending.err.is_empty());
        assert!(in_graph(&c));

        // The same bad text again changes nothing; a different one replaces it.
        assert_eq!(
            apply_server_text(&mut c, BAD, ApplyMode::Follow),
            Applied::Unchanged
        );
        assert_eq!(
            apply_server_text(&mut c, BAD2, ApplyMode::Follow),
            Applied::KeptLastGood
        );
        assert_eq!(c.pending.as_ref().unwrap().text, BAD2);
        assert_eq!(c.content, GOOD);

        // A fix clears the pending copy.
        assert_eq!(
            apply_server_text(&mut c, GOOD2, ApplyMode::Follow),
            Applied::Updated
        );
        assert_eq!(c.content, GOOD2);
        assert!(c.pending.is_none());
    }

    #[test]
    fn fixing_back_to_the_last_good_text_still_clears_pending() {
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        apply_server_text(&mut c, BAD, ApplyMode::Follow);
        assert_eq!(
            apply_server_text(&mut c, GOOD, ApplyMode::Follow),
            Applied::Updated
        );
        assert!(c.pending.is_none());
    }

    #[test]
    fn invalid_text_with_no_good_version_errors_the_card() {
        // A card that never had a good server version.
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, BAD);
        assert_eq!(
            apply_server_text(&mut c, BAD2, ApplyMode::Follow),
            Applied::NoGoodVersion
        );
        assert_eq!(c.content, BAD2);
        assert!(!c.err.is_empty());
        assert!(c.pending.is_none());
        assert!(!in_graph(&c));
        assert_eq!(
            apply_server_text(&mut c, BAD2, ApplyMode::Follow),
            Applied::Unchanged
        );

        assert_eq!(
            apply_server_text(&mut c, GOOD, ApplyMode::Follow),
            Applied::Updated
        );
        assert!(c.err.is_empty() && in_graph(&c));
    }

    #[test]
    fn first_follow_of_invalid_server_text_has_no_good_version() {
        // Local content was fine, but it is not a server version: adopting a
        // bad server copy must not keep it as "last good".
        let mut c = card("1", Some("a.yaml"), FollowState::Off, GOOD);
        c.filename = "renamed.yaml".to_string();
        assert!(needs_discard_confirm(&c, BAD));
        assert_eq!(begin_follow(&mut c, BAD, FollowState::Direct), Applied::NoGoodVersion);
        assert_eq!(c.follow, FollowState::Direct);
        assert_eq!(c.filename, "a.yaml");
        assert_eq!(c.content, BAD);
        assert!(!in_graph(&c));
        assert!(!blocks_graph(std::slice::from_ref(&c)));
    }

    #[test]
    fn follow_on_identical_is_silent_and_unchanged() {
        let mut c = card("1", Some("a.yaml"), FollowState::Off, GOOD);
        assert!(!needs_discard_confirm(&c, &format!("{}\n\n", GOOD)));
        assert_eq!(begin_follow(&mut c, GOOD, FollowState::Direct), Applied::Unchanged);
        assert_eq!(c.follow, FollowState::Direct);
    }

    #[test]
    fn follow_on_overwrites_local_edits() {
        let mut c = card("1", Some("a.yaml"), FollowState::Off, GOOD);
        assert!(needs_discard_confirm(&c, GOOD2));
        assert_eq!(begin_follow(&mut c, GOOD2, FollowState::Direct), Applied::Updated);
        assert_eq!(c.content, GOOD2);
    }

    #[test]
    fn unfollow_keeps_content_and_path() {
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        apply_server_text(&mut c, BAD, ApplyMode::Follow);
        end_follow(&mut c);
        assert_eq!(c.follow, FollowState::Off);
        assert_eq!(c.content, GOOD);
        assert_eq!(c.server_path.as_deref(), Some("a.yaml"));
        assert!(c.pending.is_none());
    }

    #[test]
    fn only_unfollowed_errors_block_the_graph() {
        let followed_broken = card("1", Some("a.yaml"), FollowState::Direct, BAD);
        let fine = card("2", None, FollowState::Off, GOOD);
        assert!(!blocks_graph(&[followed_broken.clone(), fine.clone()]));

        let local_broken = card("3", None, FollowState::Off, BAD);
        assert!(blocks_graph(&[followed_broken, fine, local_broken]));
    }

    #[test]
    fn missing_then_recovered() {
        let c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        let mut status = FetchStatus {
            fetched: true,
            ..Default::default()
        };
        assert_eq!(card_state(&c, Some(&status), true), CardState::Live);

        status.missing = true;
        assert_eq!(card_state(&c, Some(&status), true), CardState::Missing);
        assert!(card_tooltip("a.yaml", None, CardState::Missing, Some(&status)).contains("missing"));

        // The file coming back is a `changed`, which refetches it.
        let ev = ChangeEvent {
            changed: vec!["a.yaml".to_string()],
            ..Default::default()
        };
        assert_eq!(
            plan_event(std::slice::from_ref(&c), &ev, &[], &[]).refetch,
            vec!["a.yaml"]
        );
        status.missing = false;
        assert_eq!(card_state(&c, Some(&status), true), CardState::Live);
    }

    #[test]
    fn card_state_reflects_connection_and_errors() {
        let mut c = card("1", Some("a.yaml"), FollowState::Direct, GOOD);
        assert_eq!(card_state(&c, None, true), CardState::Syncing);

        let status = FetchStatus {
            fetched: true,
            updated_at: Some("12:34:56".to_string()),
            ..Default::default()
        };
        assert_eq!(card_state(&c, Some(&status), false), CardState::Stale);
        assert_eq!(
            card_tooltip("a.yaml", None, CardState::Live, Some(&status)),
            "following a.yaml · updated 12:34:56"
        );

        apply_server_text(&mut c, BAD, ApplyMode::Follow);
        assert_eq!(card_state(&c, Some(&status), true), CardState::Error);
    }

    #[test]
    fn old_storage_loads_without_follow_fields() {
        let json =
            r#"[{"id":"x","filename":"a.yaml","content":"","sims":["A"],"server_path":"a.yaml"}]"#;
        let cards: Vec<Contract> = serde_json::from_str(json).unwrap();
        assert_eq!(cards[0].follow, FollowState::Off);
        assert!(cards[0].pending.is_none());
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn under_dir_is_segment_aware_and_handles_root_and_slashes() {
        assert!(is_under_dir("a/x.yaml", "a"));
        assert!(is_under_dir("a/b/x.yaml", "a/"));
        assert!(is_under_dir("a/b/x.yaml", "/a/b/"));
        assert!(!is_under_dir("ab/x.yaml", "a"));
        assert!(!is_under_dir("ab/x.yaml", "a/"));
        assert!(!is_under_dir("a", "a"));
        assert!(!is_under_dir("a/", "a"));
        assert!(!is_under_dir("x.yaml", "a"));
        // The root holds everything.
        assert!(is_under_dir("x.yaml", ""));
        assert!(is_under_dir("a/b/x.yaml", "/"));
        assert!(!is_under_dir("", ""));
        assert_eq!(normalize_dir("/a/b/"), "a/b");
        assert_eq!(dir_label(""), "/");
        assert_eq!(dir_label("sub/"), "sub/");
    }

    #[test]
    fn root_dir_is_kept_in_the_follow_set() {
        let set = follow_set(&[], &strs(&["", "/sub/"]));
        assert_eq!(set.dirs, vec!["", "sub"]);
        assert_eq!(set.events_url().unwrap(), "/events?dir=&dir=sub");
    }

    #[test]
    fn dir_control_needs_a_containing_dir_and_no_exclusion() {
        let dirs = strs(&["a", "a/b"]);
        assert!(dir_controls("a/x.yaml", &dirs, &[]));
        assert!(!dir_controls("ab/x.yaml", &dirs, &[]));
        assert!(!dir_controls("a/x.yaml", &dirs, &strs(&["a/x.yaml"])));
        // Overlapping dirs: the innermost names the card.
        assert_eq!(controlling_dir("a/b/y.yaml", &dirs), Some("a/b"));
        assert_eq!(controlling_dir("a/y.yaml", &dirs), Some("a"));
        assert_eq!(follow_mode_for("a/y.yaml", &dirs), FollowState::ViaDir);
        assert_eq!(follow_mode_for("z.yaml", &dirs), FollowState::Direct);
    }

    #[test]
    fn dir_follow_creates_adopts_and_never_downgrades_direct() {
        let cards = vec![
            card("off", Some("d/off.yaml"), FollowState::Off, GOOD),
            card("direct", Some("d/direct.yaml"), FollowState::Direct, GOOD),
            card("via", Some("d/via.yaml"), FollowState::ViaDir, GOOD),
            card("excl", Some("d/excl.yaml"), FollowState::Off, GOOD),
        ];
        let listing = strs(&[
            "d/new2.yaml",
            "d/off.yaml",
            "d/direct.yaml",
            "d/via.yaml",
            "d/excl.yaml",
            "d/gone-excl.yaml",
            "d/new1.yaml",
            "d/new1.yaml",
        ]);
        let plan = plan_dir_listing(&cards, &listing, &strs(&["d/excl.yaml", "d/gone-excl.yaml"]));
        assert_eq!(plan.create, vec!["d/new1.yaml", "d/new2.yaml"]);
        assert_eq!(plan.adopt, vec![("off".to_string(), "d/off.yaml".to_string())]);
    }

    #[test]
    fn adopting_follows_via_dir_with_the_server_text() {
        let mut c = card("1", Some("d/a.yaml"), FollowState::Off, GOOD);
        assert!(needs_discard_confirm(&c, GOOD2));
        assert_eq!(begin_follow(&mut c, GOOD2, FollowState::ViaDir), Applied::Updated);
        assert_eq!(c.follow, FollowState::ViaDir);
        assert_eq!(c.content, GOOD2);

        let sims = strs(&["A", "B"]);
        let n = followed_card("n".into(), "d/n.yaml", GOOD, &sims, FollowState::ViaDir);
        assert_eq!(n.follow, FollowState::ViaDir);
        assert_eq!(n.filename, "d/n.yaml");
        assert_eq!(n.server_path.as_deref(), Some("d/n.yaml"));
        assert!(n.is_active_in("A") && n.is_active_in("B"));
        assert!(n.err.is_empty());

        let bad = followed_card("b".into(), "d/b.yaml", BAD, &sims, FollowState::ViaDir);
        assert!(!bad.err.is_empty() && !in_graph(&bad));
    }

    #[test]
    fn events_under_a_followed_dir_create_and_remove_cards() {
        let dirs = strs(&["d"]);
        let excl = strs(&["d/excluded.yaml"]);
        let cards = vec![
            card("via", Some("d/via.yaml"), FollowState::ViaDir, GOOD),
            card("direct", Some("d/direct.yaml"), FollowState::Direct, GOOD),
            card("off", Some("d/off.yaml"), FollowState::Off, GOOD),
        ];
        let ev = ChangeEvent {
            added: strs(&["d/new.yaml", "d/excluded.yaml", "other/x.yaml"]),
            // A PUT from elsewhere shows up as `changed`: still new here.
            changed: strs(&["d/put.yaml", "d/via.yaml", "d/off.yaml", "dx/y.yaml"]),
            removed: strs(&["d/via.yaml", "d/direct.yaml", "d/off.yaml"]),
        };
        let plan = plan_event(&cards, &ev, &dirs, &excl);
        assert_eq!(plan.create, vec!["d/new.yaml", "d/put.yaml"]);
        assert_eq!(plan.refetch, vec!["d/via.yaml"]);
        assert_eq!(plan.vanished, vec!["d/via.yaml"]);
        assert_eq!(plan.missing, vec!["d/direct.yaml"]);

        // Nothing under an unfollowed dir is created.
        assert!(plan_event(&cards, &ev, &[], &excl).create.is_empty());
        // The root follows everything.
        assert_eq!(
            plan_event(&cards, &ev, &strs(&[""]), &excl).create,
            vec!["d/new.yaml", "d/put.yaml", "dx/y.yaml", "other/x.yaml"]
        );
    }

    #[test]
    fn vanished_removes_only_via_dir_cards() {
        let mut cards = vec![
            card("via", Some("d/a.yaml"), FollowState::ViaDir, GOOD),
            card("direct", Some("d/b.yaml"), FollowState::Direct, GOOD),
            card("off", Some("d/a.yaml"), FollowState::Off, GOOD),
        ];
        assert!(remove_vanished(&mut cards, "d/a.yaml"));
        assert!(!remove_vanished(&mut cards, "d/b.yaml"));
        let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["direct", "off"]);
    }

    #[test]
    fn unfollowing_a_dir_releases_its_cards_and_prunes_exclusions() {
        let mut cards = vec![
            card("a", Some("a/x.yaml"), FollowState::ViaDir, GOOD),
            card("ab", Some("a/b/y.yaml"), FollowState::ViaDir, GOOD),
            card("direct", Some("a/z.yaml"), FollowState::Direct, GOOD),
        ];
        let mut excl = strs(&["a/b/e.yaml", "a/e.yaml"]);
        // `a` goes; `a/b` stays followed.
        let remaining = strs(&["a/b"]);
        assert_eq!(release_uncovered(&mut cards, &remaining), 1);
        prune_exclusions_to_dirs(&mut excl, &remaining);
        assert_eq!(cards[0].follow, FollowState::Off);
        assert_eq!(cards[0].content, GOOD);
        assert_eq!(cards[0].server_path.as_deref(), Some("a/x.yaml"));
        assert_eq!(cards[1].follow, FollowState::ViaDir);
        assert_eq!(cards[2].follow, FollowState::Direct);
        assert_eq!(excl, strs(&["a/b/e.yaml"]));

        // Then `a/b` too.
        assert_eq!(release_uncovered(&mut cards, &[]), 1);
        prune_exclusions_to_dirs(&mut excl, &[]);
        assert!(excl.is_empty());
        assert_eq!(cards[2].follow, FollowState::Direct);
    }

    #[test]
    fn listing_prunes_exclusions_whose_files_are_gone() {
        let mut excl = strs(&["d/gone.yaml", "d/here.yaml", "other/gone.yaml"]);
        prune_exclusions_listed(&mut excl, "d", &strs(&["d/here.yaml", "d/x.yaml"]));
        assert_eq!(excl, strs(&["d/here.yaml", "other/gone.yaml"]));
    }

    #[test]
    fn stopping_or_deleting_a_dir_card_excludes_it_and_following_again_lifts_it() {
        let dirs = strs(&["d"]);
        let mut excl: Vec<String> = Vec::new();
        let mut c = card("1", Some("d/a.yaml"), FollowState::ViaDir, GOOD);

        // Toggle off a ViaDir card: excluded, editable.
        let e = exclusion_on_release(&c, &dirs, &excl).unwrap();
        add_exclusion(&mut excl, &e);
        add_exclusion(&mut excl, &e);
        end_follow(&mut c);
        assert_eq!(excl, strs(&["d/a.yaml"]));
        assert_eq!(c.follow, FollowState::Off);
        // Already excluded: nothing more to record.
        assert_eq!(exclusion_on_release(&c, &dirs, &excl), None);
        // An excluded file changing on the server does not bring a card back.
        let ev = ChangeEvent {
            changed: strs(&["d/a.yaml"]),
            ..Default::default()
        };
        assert!(plan_event(&[], &ev, &dirs, &excl).create.is_empty());
        assert_eq!(excluded_without_card(&[], &excl), strs(&["d/a.yaml"]));
        assert!(excluded_without_card(std::slice::from_ref(&c), &excl).is_empty());

        // Re-include: lift the exclusion, follow via the dir again.
        assert!(remove_exclusion(&mut excl, "d/a.yaml"));
        assert!(!remove_exclusion(&mut excl, "d/a.yaml"));
        assert_eq!(follow_mode_for("d/a.yaml", &dirs), FollowState::ViaDir);

        // Outside any followed dir there is nothing to exclude from.
        let lone = card("2", Some("z.yaml"), FollowState::Direct, GOOD);
        assert_eq!(exclusion_on_release(&lone, &dirs, &excl), None);
        assert_eq!(exclusions_under(&strs(&["d/a.yaml", "da/b.yaml"]), "d"), vec!["d/a.yaml"]);
    }

    #[test]
    fn tooltip_names_the_dir() {
        assert_eq!(
            card_tooltip("d/a.yaml", Some("d"), CardState::Syncing, None),
            "following d/a.yaml via d/ · fetching"
        );
        assert_eq!(
            card_tooltip("a.yaml", Some(""), CardState::Syncing, None),
            "following a.yaml via / · fetching"
        );
    }
}
