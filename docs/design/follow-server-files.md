# Follow server files

**Status**: Agreed design, implementing on branch `follow-server-files`
**Date**: 2026-11-16

## Goal

Let a contract card (or a whole server directory) *follow* the contract
file(s) on the API server: the browser copy is held in sync with the file on
disk, cannot be edited locally, and updates live as the file is edited by
other tools (an editor, git checkout, etc.). Views and simulation
membership remain local, browser-side choices.

This is a base API server capability, not a server mode. There is no flag.

## Server

### `GET /events` (SSE)

- Query params name the follow set: `?file=<path>&file=<path>&dir=<path>`.
  Paths are relative to `--base-dir`, `/`-separated, same validation as
  `PUT` (no `..`, `.`, dot-names).
- The browser opens one shared stream and **reconnects whenever its follow
  set changes**. On every connect/reconnect the browser refetches everything
  it follows (via the existing `GET /contracts/*path`), so the stream never
  needs to replay history.
- Event payload (JSON, one SSE `data:` per debounced batch):
  `{"added": [...], "changed": [...], "removed": [...]}`.
  `added` only occurs for files under a followed directory.
- Served directly by the API (not via the dev proxy); keep-alive enabled.

### Watching

- Crates: `notify` 8 + `notify-debouncer-full`, native backend, ~200 ms
  debounce. (Cross-platform: inotify / FSEvents-kqueue / ReadDirectoryChangesW,
  poll fallback.) The debouncer stitches rename events so editors that save
  via temp-file + rename are handled.
- Only followed things are watched:
  - Followed **file** → watch its **parent directory non-recursively**, filter
    events to the followed files (a watch on the file itself is lost on
    atomic-rename saves).
  - Followed **directory** → recursive watch, using the same filters as the
    startup scan: `.yaml`/`.yml`, skip hidden entries, symlinks only if they
    resolve inside the base dir.
- Watches are **shared and reference-counted** across connected clients and
  dropped when the last subscriber disconnects.
- The storage path map is updated for anything the watcher observes
  (adds/removes), so newly created files in followed dirs don't 404.
  - Files created while nothing watched them are still served: `GET` adopts
    any path the startup scan would have accepted, and each new subscription
    reconciles its followed files and directories with the disk.
  - Removing a watched directory itself (often a single event for the whole
    tree) reports `removed` for every known contract under it, so a
    followed directory or a followed file's parent going away is not silent.
- `PUT /contracts/*path` is unchanged.

## Frontend

### Contract record

`Contract` gains (all `#[serde(default)]`, so existing localStorage loads):

- `follow: FollowState` — `Off | Direct | ViaDir` (default `Off`).
- Pending-bad server state: the raw server text and its error, kept apart from
  `content` (see *Keep last good*).
- Runtime-only (not persisted) status for the card: live / stale / error /
  missing, last update time, flash trigger.

New localStorage keys:

- `followed_dirs: Vec<String>`
- `follow_exclusions: Vec<String>` — global set of server paths excluded from
  *all* directory follows.

### Following a single contract

- Only cards with a `server_path` (came from / pushed to the server) show the
  follow button.
- Button sits with push/download/delete; inline Bootstrap Icons SVG
  `bi-broadcast`, outline when off, filled/`btn-info` when on.
- Turning follow on: server copy wins. If the local copy differs from the
  server, confirm ("Discard local changes to `<path>` and follow the
  server?") with Cancel; silent if identical.
- A followed card is **not locally editable**:
  - Clicking the card opens the edit modal **read-only** (no Save/Push,
    filename locked, textarea read-only).
  - Push button hidden; the "differs" area instead indicates it is following.
  - Kept: sim toggles, drag reorder, Download, Delete (local only).
- Unfollow (toggle again): card keeps last-synced content and `server_path`,
  becomes editable, normal differs check returns.
- On page load, all followed cards refetch before the graph builds. The SSE
  connection opens lazily only while ≥1 card/dir is followed.

### Keep last good

- When a followed card receives invalid server text, `content` stays at the
  last good version (the graph keeps using it); the bad text and error go in
  the pending fields; the card shows an error badge; the read-only viewer
  shows the bad text + error.
- If the very first fetch is invalid (no good version), the card shows the
  error and is left out of the graph.
- A broken followed card **never blocks** the graph rebuild (today any
  non-empty `err` blocks it; that behavior is unchanged for unfollowed cards).

### Card status

- **live**: following and SSE connected; tooltip "following `<path>` ·
  updated HH:MM:SS".
- **stale**: SSE disconnected/reconnecting (muted/warning).
- **error**: server copy invalid, showing last good.
- **missing**: file gone from server.
- Brief **flash** highlight when an update lands.

### Missing files

- A **directly** followed file that disappears: card keeps last content, stays
  followed, shows "missing"; recovers automatically if the path returns.
  Renames are not tracked (user unfollows or deletes).
- A file under a followed **directory** that disappears: its card is removed.

### Views

Open views (overview, detailed, trace, scratch groups) re-resolve in place:
node positions, selections, and folds preserved where nodes still exist; new
nodes placed incrementally; vanished nodes dropped.

## Directory following

- Server browser (`ContractBrowser`): directory rows get a **Follow** action
  alongside load. File rows also get **Follow**.
- Following a directory:
  - Creates a `ViaDir` card for every contract under it (recursive), enabled
    in all simulations, appended to the card list.
  - **Adopts** an existing card with the same `server_path` (flip to
    `ViaDir`), using the differs confirm/cancel per card.
  - Adds the dir to `followed_dirs`.
- Followed dirs render as chips above the cards: "📡 `sub/` · live" with the
  same live/stale status and an unfollow ✕. Chip shows "N excluded" when
  relevant.
- New files under a followed dir → new followed card appended, all sims, flash.
- Overlapping dirs (`a/` and `a/b/`) are allowed; cards dedupe by
  `server_path`; a card is dir-controlled while any followed dir contains it.

### Exclusions

- On a `ViaDir` card the follow toggle stays clickable: confirm "Stop
  following `x.yaml` from `sub/` so you can edit it locally?" → path added to
  `follow_exclusions`, card becomes an editable local copy (still linked by
  `server_path`, normal differs/push).
- Deleting a `ViaDir` card also adds it to `follow_exclusions` (otherwise it
  would reappear on refetch).
- Re-include:
  - Not-deleted excluded card: click follow again (differs confirm/cancel).
  - Deleted card: the server browser shows excluded paths with an "excluded"
    marker; loading or following one there removes the exclusion and it
    comes back as `ViaDir`.
- Pushing an excluded card never re-follows it.
- Pruning: when a dir is unfollowed, drop exclusions no longer under any
  followed dir; drop exclusions whose path no longer exists on the server
  when that dir refetches.

### Unfollowing a directory

- Its `ViaDir` cards stay, become `Off` (editable local copies linked by
  `server_path`).
- Cards that were `Direct` before the dir was followed keep following
  (`Direct` is not downgraded by adopting; adoption only flips `Off` →
  `ViaDir`).

## Dev workflow

- `just run-dev` (API on :8080, proxying the UI to Trunk on :3000) is the
  supported entry point for following; the API serves `/events` itself.
- Best-effort Trunk `[[proxy]]` entry for `/events` so :3000 may also work.
- CLAUDE.md documents `/events` and the :8080 note.

## Testing

- **Server**: tempdir-based tests — create, edit, atomic rename-save, delete
  produce the right `added`/`changed`/`removed`; filtering to the follow set;
  refcounted watch teardown.
- **Frontend**: follow logic as pure functions with unit tests — follow-set
  computation, adopt/exclude, exclusion pruning, keep-last-good merge.
- Manual browser checks for card states.

## Delivery

Branch `follow-server-files`, commits:

0. This design doc.
1. Server: watcher, `/events` SSE, path-map maintenance.
2. Frontend: single-file follow (field, card states, read-only modal, differs
   confirm, keep-last-good, SSE client).
3. Directory follow + exclusions (`followed_dirs` chips, exclusions, browser
   Follow/restore).

Each commit builds and passes tests. No push; fast-forward main and push after
review.

## Out of scope

- Server-wide follow mode / flags.
- Tracking renames.
