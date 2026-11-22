# Fold: Unneeded alternatives

**Status**: Implemented
**Date**: 2026-11-21

## Goal

A behavior often has more than one offer. When one of them keeps it, the
others that do not are noise: each drags its whole chain of conditions onto
the page, usually ending in something red, under a behavior that is in fact
fine. This option drops them.

It must not hide trouble. A behavior **nobody** keeps still shows every offer
for it, since those are the ways it could be fixed.

It is a display option, like Self promises, Bundle and Bound values: it never
changes resolution, identity or whether anything is satisfied. It is off by
default everywhere, and with it off every view and command prints exactly
what it did before.

- CLI flag: `--fold-alternatives`
- UI: the "Fold:" group, after "Self promises": **Unneeded alternatives**
- Fields: `ViewOptions::fold_alternatives`, `QueryOptions::fold_alternatives`

## The rule

One implementation, `Resolution::without_unneeded_alternatives` in
`src/resolve.rs`. At every node of the resolution tree:

1. If the behavior is satisfied (it has a satisfying offer), its
   unsatisfying offers are dropped, each with its whole condition subtree.
2. Each condition of every offer that remains is treated the same way.

So:

- an unsatisfied behavior keeps all its offers;
- a behavior with no offers is unchanged;
- every satisfying offer is kept, however many there are;
- an offer answering an open goal (`{{?x}}`) is an ordinary offer: candidates
  that fail go when another candidate keeps the goal, and all stay when none
  does;
- doing it twice changes nothing more, and `is_satisfied()` is the same
  before and after.

The resolution is a finite tree (cycles are cut while resolving), so the rule
is plain recursion. An offer that failed only because it ran into a cycle is
an unsatisfying offer like any other.

Known divergence: such an offer can be held when resolved on its own (`Q2`
provides `q` if `p`, `P1` provides `p` if `q`, `Q1` provides `q` outright).
The chain views then fold `Q2` away under `q`, while the Inventory, which
judges each promise from the top, still lists it as a held provider. The
cycle cut is older than this option; the option only makes it visible.

It runs first, on the tree; Self promises and Bundle then apply to what is
left, as they always have.

### Example

```
b1 |-> aforb1
   |-> i1 &-> b3 |-> a4
   |-> aforb1-2 &-> z1 |-> az1
                       |-> ay1 &-> y1 |-> ?
                &-> z2 |-> ?
```

`b1` is kept twice over, so the `aforb1-2` offer and everything under it
goes:

```
b1 |-> aforb1
   |-> i1 &-> b3 |-> a4
```

## Where it applies

| View / command | Effect |
|---|---|
| CLI `check-unsatisfied` (tree, `-c` or not) | each want's resolution is folded before printing |
| CLI `check-unsatisfied --summary` | flag accepted, changes nothing |
| CLI `simulate` | the resolution is folded before printing |
| CLI `providers` | provider rows not held are dropped when another is held |
| CLI `provides` | the agent's rows not held are dropped when the behavior is kept anyway |
| CLI `unused` | no flag |
| Overview graph | drawn from the folded resolutions (`promise_graph_with`); agents left with no promise are not drawn |
| Detailed: sequence diagram | folded before `diagram_with` |
| Detailed: text view and its export | folded in `view_rows` |
| Trace | walked from the folded resolution (`trail_with`); counts in the status strip follow |
| Inventory: Agents, Behaviors | as CLI `provides` / `providers` |
| Inventory: Unsatisfied, Unused | unaffected |

Export subtitles say "unneeded alternatives folded" only when the option is
on.

## Decisions

### Every depth

The rule holds all the way down, inside an unsatisfied want's chain too. If
`top` is broken because `p` needs `z1` and `z2`, and `z1` is kept by `az1`
while `ay1` also offers it and fails, the `ay1` offer is dropped: `z1` is not
what is wrong with `top`.

```
top |-> p &-> z1 |-> az1               top |-> p &-> z1 |-> az1
                 |-> ay1 &-> y1 |-> ?            &-> z2 |-> ?
          &-> z2 |-> ?
```

### Lone agents leave the Overview

The Overview gives every agent a node whether or not it has a promise drawn.
With the option on, an agent that has at least one edge in the unfolded graph
and none in the folded one is removed: it was only on the canvas for an offer
that is no longer shown. An agent with no edges in the unfolded graph either
stays, exactly as with the option off; the option does not start hiding
agents that were already isolated. "Missing" ghost nodes need no rule: they
exist only where an edge reaches them.

A promise two chains share is still drawn when either chain is. If `alt`
offers `b1` (broken, and `b1` is kept elsewhere) and keeps `b2`, both resting
on `c` from `cprov`, the `alt -> cprov` edge stays for `b2`.

`promise_graph_with` builds the unfolded edge set beside the folded one to
tell the two kinds of agent apart; nothing else of it is used. The nodes are
a flat list with no parent references (a folded SuperAgent is itself one
node, and goes or stays like any agent), the fold step (`src/fold.rs`) only
labels nodes that are present, and scratch groups already ignore members that
are not in the graph, so nothing is left pointing at a removed node. The
`unsatisfied` list is the same with the option on or off.

### Inventory: providers and provides honour it; unsatisfied and unused do not

- **Behaviors / `providers`**: if any provider row is held, rows that are not
  held are dropped (before bundling). If none is held, all stay. Consumers
  are unchanged.
- **Agents / `provides`**: a row of the agent that is not held is dropped
  when the behavior resolves satisfied. "Held" is the row's own flag (all of
  that promise's conditions hold); a row that is not held cannot be what
  satisfies the behavior, so a satisfied behavior means some other offer
  keeps it — another agent's, or another promise of the same agent for the
  same behavior. Held rows stay, and so do rows for behaviors nobody keeps.
  Parameterized (pattern) promises are listed as written and are not
  affected.
- **Unsatisfied**: lists only wants nobody keeps, each by its leaf causes;
  there is no kept behavior in it to fold around.
- **Unused**: an unused promise is nobody's alternative, since nothing asks
  for the behavior.

## Tests

Inline, beside the code: `src/resolve.rs` (the rule), `src/lib.rs`
(tracker-level: nested alternative under an unsatisfied want, open-variable
candidates, a cycle), `src/promise_graph.rs` (shared sub-chain, lone agents),
`src/trace.rs`, `src/query.rs`, `cli/src/check_unsatisfied.rs` (including the
`samples/` directory's lone agents), and the frontend's `display_options.rs`
and `contract_text.rs`.
