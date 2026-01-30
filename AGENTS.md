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
- `components/` - Domain models: `Agent`, `Behavior`, `SuperAgent`, `Item`

### API Server (`api/`)
- REST endpoints: `GET /api/contracts`, `GET /api/contracts/{id}`
- Static file serving for frontend
- Dev mode proxies to Trunk server (port 3000)

### Frontend (`frontend/`)
- Leptos 0.7 reactive framework
- Browser localStorage for contract persistence
- Key components: `contract_browser.rs`, `contract_edit_modal.rs`, `contract_grapher.rs`

### CLI (`cli/`)
Commands: `agents`, `behaviors`, `check-unsatisfied`, `simulate`, `validate`, `who-provides`, `schema`

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
