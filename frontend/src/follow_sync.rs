//! Following server contract files: the browser half.
//!
//! One app-wide [`FollowSync`] owns the shared `/events` stream and every
//! fetch of a followed contract. It is provided as Leptos context by `App`.
//! The decisions themselves (follow set, event planning, keep-last-good) are
//! the pure functions in [`crate::follow`].
//!
//! - The stream is open only while the follow set is non-empty, and is
//!   reopened whenever the set changes (a `Memo`, so unrelated card edits do
//!   not reconnect it).
//! - Every open, first or reconnect, refetches every followed contract: the
//!   stream does not replay what happened while it was down.
//! - On an error the stream is closed and reopened by us with backoff
//!   (1 s doubling to 30 s), rather than left to the browser's own retry,
//!   which gives up for good on a non-200 answer.
//! - On page load every followed card is fetched straight away, without
//!   waiting for the stream, and `ready` turns true once each has answered
//!   (or after a short timeout). The graph does not build until then, so it
//!   never draws the persisted copies only to redraw them a moment later.
//!
//! Directory following adds `followed_dirs` to the follow-set memo and new
//! arms to the event handler (new cards for `added`, removing `ViaDir` cards
//! on `removed`); the connection handling stays as it is.

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{EventSource, MessageEvent};

use crate::follow::{
    apply_server_text, begin_direct_follow, end_follow, follow_set, followed_paths,
    needs_discard_confirm, plan_event, ApplyMode, ChangeEvent, FetchStatus,
};
use crate::models::{Contract, FollowState};
use crate::utils::fetch_server_contract;

/// How long the first graph build waits on followed cards at most.
const READY_TIMEOUT_MS: u32 = 4_000;
const BACKOFF_START_MS: u32 = 1_000;
const BACKOFF_MAX_MS: u32 = 30_000;

/// App-wide follow state and actions. All fields are signals, so it is `Copy`.
#[derive(Clone, Copy)]
pub struct FollowSync {
    contracts: ReadSignal<Vec<Contract>>,
    set_contracts: WriteSignal<Vec<Contract>>,
    /// Per server path, what this session knows about the server copy.
    pub statuses: RwSignal<HashMap<String, FetchStatus>>,
    /// The event stream is open.
    pub connected: RwSignal<bool>,
    /// Followed cards have had their first fetch (or it timed out).
    pub ready: RwSignal<bool>,
    /// The newest fetch issued per path; older answers are dropped.
    issued: StoredValue<HashMap<String, u64>>,
}

impl FollowSync {
    /// Set up following for these cards: initial fetches and the stream.
    pub fn start(
        contracts: ReadSignal<Vec<Contract>>,
        set_contracts: WriteSignal<Vec<Contract>>,
    ) -> Self {
        let sync = FollowSync {
            contracts,
            set_contracts,
            statuses: RwSignal::new(HashMap::new()),
            connected: RwSignal::new(false),
            ready: RwSignal::new(false),
            issued: StoredValue::new(HashMap::new()),
        };

        // Initial fetch of everything followed, before the graph builds.
        let startup = followed_paths(&contracts.get_untracked());
        if startup.is_empty() {
            sync.ready.set(true);
        } else {
            for path in &startup {
                sync.refetch(path.clone());
            }
            Effect::new(move |_| {
                if sync.ready.get_untracked() {
                    return;
                }
                let statuses = sync.statuses.get();
                let answered = startup.iter().all(|p| {
                    statuses
                        .get(p)
                        .map(|s| s.fetched || s.fetch_error.is_some())
                        .unwrap_or(false)
                });
                if answered {
                    sync.ready.set(true);
                }
            });
            Timeout::new(READY_TIMEOUT_MS, move || sync.ready.set(true)).forget();
        }

        // The stream follows the follow set.
        let set = Memo::new(move |_| follow_set(&contracts.get(), &[]));
        let conn: Rc<RefCell<Conn>> = Rc::new(RefCell::new(Conn::default()));
        Effect::new(move |_| {
            let url = set.get().events_url();
            let mut c = conn.borrow_mut();
            c.generation += 1;
            c.retry = None;
            c.stream = None;
            c.backoff_ms = BACKOFF_START_MS;
            c.url = url.clone();
            sync.connected.set(false);
            drop(c);
            if url.is_some() {
                open_stream(conn.clone(), sync);
            }
        });

        sync
    }

    /// Number a new fetch of `path`; only the newest number may land.
    fn issue(self, path: &str) -> u64 {
        let seq = self
            .issued
            .with_value(|m| m.get(path).copied().unwrap_or(0))
            + 1;
        self.issued.update_value(|m| {
            m.insert(path.to_string(), seq);
        });
        seq
    }

    /// No fetch of `path` was issued after `seq`.
    fn is_latest(self, path: &str, seq: u64) -> bool {
        self.issued.with_value(|m| m.get(path).copied()) == Some(seq)
    }

    /// Fetch `path` for a one-off action (following, a new card), through
    /// the same sequence as `refetch`. An answer overtaken by a newer fetch
    /// of the path is not used; the fetch is issued again (which in turn
    /// stands the other one down), so the action lands the newest text. A
    /// path that keeps changing gets the last answer after a few tries.
    async fn fetch_latest(self, path: &str) -> Result<Option<String>, String> {
        const TRIES: usize = 4;
        let mut tries = 0;
        loop {
            tries += 1;
            let seq = self.issue(path);
            let result = fetch_server_contract(path).await;
            if self.is_latest(path, seq) || tries == TRIES {
                return result;
            }
        }
    }

    /// Fetch one server path again and land it on every card following it.
    pub fn refetch(self, path: String) {
        let seq = self.issue(&path);

        spawn_local(async move {
            let result = fetch_server_contract(&path).await;
            // A newer fetch of the same path is in flight; let it land instead.
            if !self.is_latest(&path, seq) {
                return;
            }
            match result {
                Ok(Some(text)) => {
                    let mut list = self.contracts.get_untracked();
                    let mut landed = false;
                    for c in list.iter_mut().filter(|c| {
                        c.follow != FollowState::Off && c.server_path.as_deref() == Some(&path)
                    }) {
                        let applied = apply_server_text(c, &text, ApplyMode::Follow);
                        landed |= applied.landed();
                    }
                    if landed {
                        self.set_contracts.set(list);
                    }
                    self.set_status(&path, |s| {
                        s.fetched = true;
                        s.missing = false;
                        s.fetch_error = None;
                        s.updated_at = Some(now_hms());
                        if landed {
                            s.flash += 1;
                        }
                    });
                }
                Ok(None) => self.set_status(&path, |s| {
                    s.fetched = true;
                    s.missing = true;
                    s.fetch_error = None;
                }),
                Err(e) => self.set_status(&path, |s| s.fetch_error = Some(e)),
            }
        });
    }

    /// Refetch everything followed (on every stream open).
    fn refetch_all(self) {
        for path in followed_paths(&self.contracts.get_untracked()) {
            self.refetch(path);
        }
    }

    /// Act on one event from the stream.
    fn handle_event(self, event: &ChangeEvent) {
        let plan = plan_event(&self.contracts.get_untracked(), event);
        for path in plan.refetch {
            self.refetch(path);
        }
        for path in plan.missing {
            self.set_status(&path, |s| {
                s.missing = true;
                s.updated_at = Some(now_hms());
            });
        }
    }

    fn set_status(self, path: &str, f: impl FnOnce(&mut FetchStatus)) {
        self.statuses
            .update(|m| f(m.entry(path.to_string()).or_default()));
    }

    /// The follow button on a card: start or stop following its server path.
    pub fn toggle(self, contract_id: String) {
        let Some(card) = self
            .contracts
            .get_untracked()
            .into_iter()
            .find(|c| c.id == contract_id)
        else {
            return;
        };

        match card.follow {
            FollowState::Off => {
                let Some(path) = card.server_path.clone() else {
                    return;
                };
                spawn_local(async move { self.begin(contract_id, path).await });
            }
            // A `ViaDir` card's toggle becomes an exclusion with directory
            // following; until then both kinds simply stop.
            FollowState::Direct | FollowState::ViaDir => {
                self.set_contracts.update(|list| {
                    if let Some(c) = list.iter_mut().find(|c| c.id == contract_id) {
                        end_follow(c);
                    }
                });
            }
        }
    }

    async fn begin(self, contract_id: String, path: String) {
        let window = web_sys::window().expect("no global window exists");
        let text = match self.fetch_latest(&path).await {
            Ok(Some(text)) => text,
            Ok(None) => {
                let _ = window.alert_with_message(&format!(
                    "{} is not on the server, so there is nothing to follow.",
                    path
                ));
                return;
            }
            Err(e) => {
                let _ = window.alert_with_message(&format!("Could not follow {}: {}", path, e));
                return;
            }
        };

        // The card may have changed (or gone) while the fetch was out.
        let Some(card) = self
            .contracts
            .get_untracked()
            .into_iter()
            .find(|c| c.id == contract_id)
        else {
            return;
        };
        if card.follow != FollowState::Off {
            return;
        }
        if needs_discard_confirm(&card, &text) {
            let ok = window
                .confirm_with_message(&format!(
                    "Discard local changes to {} and follow the server?",
                    path
                ))
                .unwrap_or(false);
            if !ok {
                return;
            }
        }

        let mut landed = false;
        self.set_contracts.update(|list| {
            if let Some(c) = list.iter_mut().find(|c| c.id == contract_id) {
                landed = begin_direct_follow(c, &text).landed();
            }
        });
        self.set_status(&path, |s| {
            s.fetched = true;
            s.missing = false;
            s.fetch_error = None;
            s.updated_at = Some(now_hms());
            if landed {
                s.flash += 1;
            }
        });
    }
}

/// The live stream and its handlers; dropping it closes the stream.
struct Stream {
    source: EventSource,
    _on_open: Closure<dyn FnMut(web_sys::Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.source.close();
    }
}

#[derive(Default)]
struct Conn {
    /// Bumped on every follow-set change; handlers of an older one stand down.
    generation: u64,
    url: Option<String>,
    stream: Option<Stream>,
    retry: Option<Timeout>,
    backoff_ms: u32,
}

fn open_stream(conn: Rc<RefCell<Conn>>, sync: FollowSync) {
    let (generation, url) = {
        let c = conn.borrow();
        match c.url.clone() {
            Some(url) => (c.generation, url),
            None => return,
        }
    };

    let source = match EventSource::new(&url) {
        Ok(source) => source,
        Err(e) => {
            web_sys::console::error_1(&e);
            schedule_retry(conn, sync, generation);
            return;
        }
    };

    let on_open = {
        let conn = conn.clone();
        Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let mut c = conn.borrow_mut();
            if c.generation != generation {
                return;
            }
            c.backoff_ms = BACKOFF_START_MS;
            drop(c);
            sync.connected.set(true);
            sync.refetch_all();
        })
    };

    let on_message = {
        let conn = conn.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
            if conn.borrow().generation != generation {
                return;
            }
            let Some(data) = ev.data().as_string() else {
                return;
            };
            match ChangeEvent::parse(&data) {
                Ok(event) => sync.handle_event(&event),
                Err(e) => {
                    web_sys::console::error_1(&format!("Bad follow event {:?}: {}", data, e).into())
                }
            }
        })
    };

    let on_error = {
        let conn = conn.clone();
        Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            {
                let c = conn.borrow();
                if c.generation != generation {
                    return;
                }
                // Close now, but leave the Stream (and this closure) in place
                // until the retry replaces it: it must not drop itself.
                if let Some(stream) = &c.stream {
                    stream.source.close();
                }
            }
            sync.connected.set(false);
            schedule_retry(conn.clone(), sync, generation);
        })
    };

    source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    source.set_onerror(Some(on_error.as_ref().unchecked_ref()));

    conn.borrow_mut().stream = Some(Stream {
        source,
        _on_open: on_open,
        _on_message: on_message,
        _on_error: on_error,
    });
}

fn schedule_retry(conn: Rc<RefCell<Conn>>, sync: FollowSync, generation: u64) {
    let mut c = conn.borrow_mut();
    if c.generation != generation || c.retry.is_some() {
        return;
    }
    let delay = c.backoff_ms.max(BACKOFF_START_MS);
    c.backoff_ms = (delay * 2).min(BACKOFF_MAX_MS);
    let conn_for_retry = conn.clone();
    c.retry = Some(Timeout::new(delay, move || {
        {
            let mut c = conn_for_retry.borrow_mut();
            if c.generation != generation {
                return;
            }
            // This callback belongs to that Timeout: let it go without
            // dropping it from inside its own call.
            if let Some(timeout) = c.retry.take() {
                timeout.forget();
            }
            c.stream = None;
        }
        open_stream(conn_for_retry, sync);
    }));
}

/// The local time now, as "HH:MM:SS".
fn now_hms() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "{:02}:{:02}:{:02}",
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds()
    )
}
