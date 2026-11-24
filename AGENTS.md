# Promise Tracker

A visualization tool for Promise Theory contracts, built entirely in Rust.

**Status**: Work in Progress
**Language**: Rust (100%)
**Architecture**: Cargo workspace monorepo

## Quick Start

Two-shell development workflow:
```bash
# Shell 1: API server (port 8080)
just run-dev

# Shell 2: Frontend dev server (port 3000)
just dev-leptos
```

## Project Structure

```
promise-tracker/
├── src/           # Core library - domain logic (Tracker, Agent, Behavior, Resolution)
├── api/           # REST API server (Axum + Tokio, port 8080)
├── cli/           # Command-line interface
├── frontend/      # Leptos web UI (WASM, compiles with Trunk)
├── wpt/           # WASM bindings package (legacy, for old React frontend)
├── samples/       # Sample contract YAML files
└── .justfile      # Build automation commands
```

## Build Commands

```bash
just build-backend    # Build API server (release)
just run-dev          # Run API in dev mode
just build-leptos     # Build frontend (release)
just dev-leptos       # Run frontend dev server
just build-wasm       # Build legacy WASM package
```

## Key Components

### Core Library (`src/`)
- `lib.rs` - Main `Tracker` struct for managing agents and resolution
- `components/pattern.rs` - Parameterized names: `match_ground` (a pattern
  against a concrete name) and `unify` (two-sided, for declared variables,
  with the "wording must correspond" rule in one private type)
- `resolve.rs` - Resolution logic (who provides what).
  `Resolution::without_unneeded_alternatives` is the one rule behind the
  "Fold: Unneeded alternatives" display option (`--fold-alternatives`): at
  every depth, a behavior that is kept loses its offers that are not kept,
  each with everything it rests on; a behavior nobody keeps keeps them all.
  Display only (see `docs/design/fold-alternatives.md`)
- `query.rs` - Inventory queries as data: an agent's provides, a behavior's
  providers/consumers, unsatisfied wants with leaf causes (a condition still
  holding a declared variable is `CauseKind::Unfound`, naming the variable
  and any promise that only held for any value), unused promises.
  `QueryOptions::fold_alternatives` drops promises that are not held, for a
  behavior something else keeps, from a behavior's providers and an agent's
  provides; unsatisfied and unused ignore it
- `marks.rs` - Where substituted values sit in behavior names, for the "Mark:
  Bound values" display option (labels only, never identity): by agent and
  name for the Overview graph, and by want, by one promise and by one
  promise's condition for the Inventory and CLI rows. A trace carries its own
  spans (`Trail::labels`); the text and sequence views read them off the
  resolution
- `components/` - Domain models: `Agent`, `Behavior`, `SuperAgent`, `Item`

### API Server (`api/`)
- REST endpoints (served at the root, not under `/api`):
  - `GET /contracts` - list the base directory (JSON, or HTML for browsers)
  - `GET /contracts/{path}` - a contract's YAML, or a subdirectory listing
  - `PUT /contracts/{path}` - validate and save a contract
  - `GET /events?file=<path>&file=<path>&dir=<path>` - Server-Sent Events for
    followed contracts (`file`/`dir` repeatable; paths relative to
    `--base-dir`, `/`-separated, no `..`/`.`/dot-names; `dir=` alone follows
    the whole base dir). One event per ~200 ms debounced batch, data:
    `{"added":[...],"changed":[...],"removed":[...]}` (sorted relative paths;
    `added` only for files under a followed `dir`). No history replay:
    clients refetch via `GET /contracts/{path}` on (re)connect. Implemented in
    `api/src/watch.rs` (notify + notify-debouncer-full; watches shared and
    refcounted across clients).
- Use following via http://localhost:8080 with `just run-dev`; the API serves
  `/events` itself (Trunk's :3000 proxy for it is best-effort only).
- Static file serving for frontend
- Dev mode proxies to Trunk server (port 3000)

### Frontend (`frontend/`)
- Leptos 0.7 reactive framework
- Browser localStorage for contract persistence
- Key components: `contract_browser.rs`, `contract_edit_modal.rs`, `contract_grapher.rs`

### CLI (`cli/`)
Commands: `agents`, `behavior`, `check-unsatisfied`, `provides` (alias
`who-provides`), `providers <behavior>`, `unused`, `simulate`, `validate`,
`schema`. Display flags matching the UI toggles, where meaningful:
`--unfold` (SuperAgent members as agents), `--fold-self`, `--bundle`;
`check-unsatisfied --summary` lists unsatisfied wants with leaf causes.
`validate` also runs the checks that need every file: membership cycles,
dangling instance bases, non-ground wants, and declared variables that can
be anything (`Tracker::unnamable_variables`).
`--mark-bound` on `provides` / `providers` / `unused` wraps substituted
values in `<>` (see `docs/design/self-and-open-variables.md`).
`--fold-alternatives` on `check-unsatisfied` / `simulate` / `providers` /
`provides` leaves out offers that are not kept where another offer keeps the
same behavior (a no-op with `check-unsatisfied --summary`; `unused` has no
such flag; see `docs/design/fold-alternatives.md`).
`provides` / `providers` / `unused` / `check-unsatisfied --summary` are thin
printers over `src/query.rs`, which the UI's Inventory tab shares.

## Contract Format

YAML files with multi-document support:
```yaml
kind: SuperAgent
name: my-cluster
---
kind: Agent
name: web-server
provides:
  - http-service
wants:
  - database-connection
```

Key concepts:
- **Agent**: Entity that makes promises (provides) and has needs (wants)
- **SuperAgent**: Groups of agents with optional instances/tags
- **Behavior**: Named promises with optional conditions
- **`{{!self}}`** (or `{{!SELF}}`): in a behavior name, condition or want,
  replaced with the declaring agent's own name (the instance's name for an
  Instance, the member's for a SuperAgent member) before resolution
- **`forSome: [x]`**: on a promise (never a want), declares variables its
  conditions find values for, written plainly as `{{x}}`; a condition
  variable must be in the promise name or in `forSome`. Values are found by
  two-sided matching against promises outright or parameterized
  (`Pattern::unify`; with variables on both sides the wording must
  correspond). A variable used by two or more conditions gives one offer per
  candidate value; one used by a single condition makes that condition one
  row gathering every match. Resolution stays concrete. A declared variable
  only a holds-for-any promise answers is invalid (`validate`). The old
  `{{?x}}` is a validation error that spells out this rewrite. See
  `docs/design/self-and-open-variables.md` §3, §5

## Technology Stack

- **Backend**: Axum 0.7, Tokio, serde, serde_yaml
- **Frontend**: Leptos 0.7, wasm-bindgen, web-sys, gloo-net
- **CLI**: clap 4.0, colored

## Known Issues

- `todo!()` panic in `src/lib.rs:159`
- `.unwrap()` calls in WASM bindings need error handling
- Limited test coverage (basic unit tests only)
- No integration tests

## Testing

Run tests:
```bash
cargo test                    # All workspace tests
cargo test -p promise_tracker # Core library only
cargo test -p api             # API tests only
```

Tests are inline in source files using `#[cfg(test)]` modules.

## Data Flow

```
YAML Contract → Tracker.add_contract() → Agent/SuperAgent creation
             → Tracker.resolve() → Resolution (who provides what)
             → Diagrams/Queries
```
