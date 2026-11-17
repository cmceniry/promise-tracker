//! Following server contract files: the pure half.
//!
//! Everything here decides *what* should happen when the server's copy of a
//! followed contract moves — which paths the event stream should name, which
//! cards an event touches, how a fetched text lands on a card, what a card's
//! status reads as. None of it touches the browser, so it is unit tested on
//! the host with `cargo test -p promise_tracker_frontend`. The browser half
//! (EventSource, fetches, signals) lives in `follow_sync.rs` and calls in here.
//!
//! Directory following extends this module: `FollowSet::dirs`,
//! `FollowState::ViaDir`, and the `added` half of an event are already carried
//! through so that only the planning rules need to grow.

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

    let mut dirs: Vec<String> = followed_dirs
        .iter()
        .filter(|d| !d.trim().is_empty())
        .cloned()
        .collect();
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
}

/// Plan the response to an event. Paths nobody follows are ignored, so a
/// stale event racing a follow-set change does no harm.
pub fn plan_event(contracts: &[Contract], event: &ChangeEvent) -> EventPlan {
    let followed = |path: &String| {
        contracts
            .iter()
            .any(|c| c.follow != FollowState::Off && c.server_path.as_ref() == Some(path))
    };
    let direct = |path: &String| {
        contracts
            .iter()
            .any(|c| c.follow == FollowState::Direct && c.server_path.as_ref() == Some(path))
    };

    let mut refetch: Vec<String> = event
        .added
        .iter()
        .chain(event.changed.iter())
        .filter(|p| followed(p))
        .cloned()
        .collect();
    refetch.sort();
    refetch.dedup();

    let mut missing: Vec<String> = event
        .removed
        .iter()
        .filter(|p| direct(p))
        .cloned()
        .collect();
    missing.sort();
    missing.dedup();

    EventPlan { refetch, missing }
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

/// Turn direct following on, with the server's text already in hand.
pub fn begin_direct_follow(contract: &mut Contract, server_text: &str) -> Applied {
    if let Some(path) = contract.server_path.clone() {
        // The filename is locked while following; it is the server's.
        contract.filename = path;
    }
    contract.follow = FollowState::Direct;
    contract.pending = None;
    apply_server_text(contract, server_text, ApplyMode::Adopt)
}

/// Stop following: the card keeps its content and `server_path` and becomes
/// an ordinary, editable card again.
pub fn end_follow(contract: &mut Contract) {
    contract.follow = FollowState::Off;
    contract.pending = None;
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
pub fn card_tooltip(path: &str, state: CardState, status: Option<&FetchStatus>) -> String {
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
        let plan = plan_event(&cards, &ev);
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
        assert_eq!(begin_direct_follow(&mut c, BAD), Applied::NoGoodVersion);
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
        assert_eq!(begin_direct_follow(&mut c, GOOD), Applied::Unchanged);
        assert_eq!(c.follow, FollowState::Direct);
    }

    #[test]
    fn follow_on_overwrites_local_edits() {
        let mut c = card("1", Some("a.yaml"), FollowState::Off, GOOD);
        assert!(needs_discard_confirm(&c, GOOD2));
        assert_eq!(begin_direct_follow(&mut c, GOOD2), Applied::Updated);
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
        assert!(card_tooltip("a.yaml", CardState::Missing, Some(&status)).contains("missing"));

        // The file coming back is a `changed`, which refetches it.
        let ev = ChangeEvent {
            changed: vec!["a.yaml".to_string()],
            ..Default::default()
        };
        assert_eq!(
            plan_event(std::slice::from_ref(&c), &ev).refetch,
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
            card_tooltip("a.yaml", CardState::Live, Some(&status)),
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
}
