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
- `resolve.rs` - Resolution logic (who provides what)
- `query.rs` - Inventory queries as data: an agent's provides, a behavior's
  providers/consumers, unsatisfied wants with leaf causes, unused promises
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
