# Inventory tab

**Status**: Agreed design, implementing on `main`
**Date**: 2026-11-17

## Goal

Bring the CLI's contract queries into the UI, and add the two that are
missing from both:

1. What an agent (or SuperAgent) provides — the CLI's `who-provides`.
2. Which agents provide a behavior — new.
3. Which wants are unsatisfied — the CLI's `check-unsatisfied`.
4. Which provided behaviors nobody uses — new.

All four respect the display options (SuperAgents / Bundle / Self) and update
live with followed files.

## Placement

A fourth tab, **Inventory**, after Overview / Detailed View / Trace. Like the
other tabs it shows one panel per visible simulation, side by side, under the
shared `DisplayOptions` bar (`ViewOptions`). Each panel reads its
simulation's tracker signal, so following needs nothing extra: when a followed
file changes the tracker is rebuilt and the panel re-renders. No "changed
since last update" highlighting.

Inside the tab, four inner tabs:

| Inner tab | Item | Badge |
|---|---|---|
| **Agents** | 1 | — |
| **Behaviors** | 2 | — |
| **Unsatisfied** | 3 | count, e.g. `Unsatisfied (1)` |
| **Unused** | 4 | count |

The inner tab, the selected agent and the selected behavior are shared across
all simulation panels (as the Trace tab shares its selection), so every panel
shows the same subject. If the selected agent or behavior does not exist in a
simulation, that panel says "not present in this simulation".

## The four views

### Agents (what an agent provides)

- Searchable agent picker (agent names as the tracker currently presents
  them: SuperAgents when folded, members when unfolded).
- The agent's provides, one row per behavior. A conditional provide lists its
  conditions, each marked held / broken (resolved against the tracker).
- Pattern provides are listed apart and marked as patterns.

### Behaviors (who provides a behavior)

- Searchable behavior picker.
- **Providers**: each agent offering the behavior, unconditional or
  conditional; conditional ones list their conditions held / broken.
- **Consumers**: each agent that wants the behavior, and each agent that needs
  it in a condition (noting which of its provides carries that condition).

### Unsatisfied

- One row per unsatisfied want (agent + behavior). Want-level only: a
  conditional offer that fails while another provider still covers the want
  is not listed.
- Each row shows the leaf cause(s): a behavior nobody provides, or a
  condition that does not hold (down to the first missing behavior).
- Each row has a **trace** link that opens the Trace tab on that want (sets
  the existing `d_component` / `d_behavior` signals and switches tab).

### Unused

- A provided behavior is **unused** when no agent wants it and no condition
  anywhere needs it.
- A behavior wanted / conditioned only by the agent that provides it still
  counts as **used** (but see Self below).
- A pattern provide is unused when it matches no wanted or conditioned
  behavior.
- Grouped by behavior, with its providers listed under it.

## Display options

### SuperAgents (`collectives`)

Folded: the tracker presents SuperAgents in place of their members (existing
`Tracker::set_fold_collectives`), so every list shows SuperAgent names.
Unfolded: members appear as agents of their own.

Selection follows the fold:

- Agent selected and SuperAgents turned **on**, and the selected agent is a
  member: the selection moves to the SuperAgent that folds it. Remember the
  original member.
- SuperAgent selected and SuperAgents turned **off**: if the selection got
  there by folding a remembered member, return to that member. Otherwise show
  the SuperAgent's members as a clickable list in place of the provides list.

### Bundle (`bundle`)

Rows that differ only by agent collapse into one row with a count, e.g.
`http-service ×4 providers`; clicking expands it to list the agents. Applies
wherever a list repeats per agent (Behaviors' providers / consumers, Unused's
providers, Unsatisfied rows sharing a behavior and cause).

### Self (`fold_self`)

A **self-promise** is agent A providing behavior X where A is the *only*
agent that wants X or needs X in a condition. If any other agent uses X it is
not a self-promise.

With Self on, self-promises disappear entirely:

- **Agents**: A's self-only provides are dropped from A's list.
- **Behaviors**: A is not listed as a consumer of its own X; A stays a
  provider while some other agent uses X. If A is X's only user, the view
  shows nothing for X.
- **Unsatisfied**: unaffected (a self-satisfied want is never unsatisfied).
- **Unused**: self-promises are *not* reported as unused — they are folded
  away, not unused.

## Navigation

Two click behaviours:

- **Peek**: clicking an agent or behavior name inside a list opens a small
  popover with a summary (agent: provide / want counts and held / broken
  status; behavior: providers and consumers) without leaving the view. The
  popover has an **Open** action that navigates.
- **Navigate**: picker results and the Unsatisfied **trace** link navigate on
  a plain click. Open on an agent → Agents inner tab with it selected; on a
  behavior → Behaviors inner tab; on an unsatisfied want → Trace tab.

No linking to the Overview graph in this round.

## Core library and CLI

The queries live in a new core module, `src/query.rs`, with unit tests, used
by both the UI and the CLI so the two cannot drift. The module takes the
tracker plus the options that affect lists (self folding, bundling);
SuperAgent folding is already a property of the tracker.

CLI:

- `who-provides` is renamed `provides` (`who-provides` kept as an alias).
- New `providers <behavior>` and `unused` commands.
- Flags matching the UI toggles, defaulting to today's CLI behaviour:
  `--unfold` (show SuperAgent members), `--fold-self`, `--bundle`.

## Out of scope

- Linking from the Inventory to the Overview graph.
- Export / download of the Inventory views.
- Highlighting changes from followed files.

## Implementation plan

Commits on `main`, one per step, fixes folded in before each commit:

1. Core module `src/query.rs` + tests + CLI commands and flags.
2. Frontend Inventory tab.
3. Independent validation: `cargo test`, diff review against this document,
   and driving the UI on `samples/` and `examples/kubernetes` with each toggle
   combination and a followed file.
