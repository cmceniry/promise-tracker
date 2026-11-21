# Self-Reference and Open Variables

**Status**: phases 1 and 2 implemented; of phase 3, sideways flow (a value
shared between conditions) is implemented and upward flow is not started
**Date**: 2026-11-19
**Scope**: core library, CLI, frontend
**Builds on**: `docs/design/parameterized-behaviors.md` (P0–P3 landed)

Three additions to parameterized behavior names, in the order they will land:

| Phase | Addition | State of the definition |
|---|---|---|
| 1 | `{{!self}}` — an agent refers to itself by a reserved variable | specified here, implemented |
| 2 | `{{?x}}` — an open variable, bound by whoever provides a match | specified here, implemented |
| 3 | Binding direction — a value bound on the provider's side flows back up | open problem, §5 |

## 1. Problem

One pattern should be usable in several contexts. The motivating chain: A1
wants to read R1; the owning service S1 grants it if its operator O1 has
configured it; O1 configures it if a design doc shows the usage.

```yaml
kind: Agent
name: A1
wants:
- name: A1 reads R1
---
kind: Agent
name: S1
provides:
- name: A1 reads R1
  conditions:
    - S1 configured to allow R1 read access for A1
---
kind: Agent
name: O1
provides:
- name: S1 configured to allow R1 read access for A1
  conditions:
    - D1 shows usage of R1 by A1
---
kind: Agent
name: D1
provides:
- name: D1 shows usage of R1 by A1
```

Today's patterns already remove one kind of repetition: S1 and O1 can write
`{{agent}}` for A1 and the wanter's ground name binds it down the chain. Two
things remain that they cannot do.

- **An agent cannot refer to itself.** A1 and D1 spell their own names inside
  their behaviors. A rename means editing every behavior, and a definition
  cannot be reused under another name (an Instance base, most obviously).
- **A consumer cannot be indifferent to who provides.** O1 has to name D1 in
  its condition. What O1 needs is that *some* document shows the usage; which
  one is not O1's business, and O1 should not be edited when D2 replaces D1.

With both additions the chain becomes:

```yaml
kind: Agent
name: A1
wants:
- name: "{{!self}} reads R1"
---
kind: Agent
name: S1
provides:
- name: "{{agent}} reads R1"
  conditions:
    - "{{!self}} configured to allow R1 read access for {{agent}}"
---
kind: Agent
name: O1
provides:
- name: "S1 configured to allow R1 read access for {{agent}}"
  conditions:
    - "{{?doc}} shows usage of R1 by {{agent}}"
---
kind: Agent
name: D1
provides:
- name: "{{!self}} shows usage of R1 by A1"
```

### Considered and dropped: `{{wanter}}` and `{{provider}}` as roles

A reserved `{{wanter}}` that must equal the name of the agent actually asking
was rejected. `Tracker::resolve` takes only a goal string and caches by it;
a checked role would make every answer depend on the asker. It also yields the
wrong value where it is needed: when O1's promise is reached as S1's
condition, the wanter is S1, not A1. A1 reaches O1 only through an ordinary
variable. `{{wanter}}` remains usable as an ordinary variable name with no
special meaning.

The underlying question — which occurrence of a variable is the one that binds
it — is phase 3.

## 2. Phase 1 — `{{!self}}`

### Semantics

- `!` marks a reserved built-in namespace. `{{!self}}` and `{{!SELF}}` are
  exact synonyms. Any other `{{!…}}` is a validation error. `!` is not a legal
  variable character today, so no existing contract collides.
- `{{!self}}` is replaced with the declaring document's own name when the
  tracker builds its working agents, before any resolution. It is allowed in
  provides names, conditions and wants.
- After substitution it is plain text. A want `{{!self}} reads R1` is ground,
  and resolution, the graph and the queries are unchanged.
- It is never bound by matching and cannot be supplied by an Instance's
  `bindings:`. It is not allowed inside a `bindings:` value in this phase.

### Whose name

| Written in | `{{!self}}` is |
|---|---|
| an Agent standing on its own | the agent's name |
| an Instance's own provides/wants | the instance's name |
| a base, materialized through an Instance | the instance's name |
| an Agent that is a member of a SuperAgent | the member's own name, folded or unfolded |

The Instance rule means a base that wants `{{!self}} reads R1` yields a
distinct want per instance with no `bindings:` at all. The member rule means
behavior text does not change under `--unfold`.

### Known wrinkle: instanced SuperAgents overlap

If a SuperAgent is the base of two Instances (`prod`, `staging`), a member
`db` writing `{{!self}} reads R1` produces `db reads R1` in both. This is
accepted for now: the rule stays "self is the name of the document that wrote
it", and the author adds a binding when the copies must differ
(`{{!self}}/{{env}} reads R1`). Revisit if it bites in real contracts.

### Display

Every tool shows the substituted text. `{{!self}}` is visible only in the YAML
editor.

An optional marking wraps substituted values in `<>`: `<A1> reads R1`.

- Display only, off by default. The behavior's identity is the plain text;
  `<A1> reads R1` and `A1 reads R1` are the same behavior.
- A UI toggle in the options bar ("Mark: Bound values") and a `--mark-bound`
  flag on CLI `provides`.
- Shown wherever one agent's own declaration is listed: the Inventory tab's
  Agents detail (promises, their conditions, patterns), the trace wants list,
  and CLI `provides`. CLI `agents` and `behavior` list names that belong to no
  single declaration, so they carry no marking.
- Not on graph edge labels. An edge joins two declarations of one name, which
  may have been written differently, so there is no single right marking.
- Covers every substituted value. Phase 1 delivers it for `{{!self}}` and
  Instance `bindings:`, both fixed at build time. Values bound by matching a
  goal (`<A1> reads R1` against `{{agent}} reads R1`) follow in phase 2.

### Validation

Per document (`src/validate.rs`):

- `{{!x}}` where `x` is not `self` / `SELF`
- `{{!self}}` inside an Instance `bindings:` value (already rejected, as any
  `{{` is)

### Implementation sketch

As built:

- `pattern.rs`: `{{!self}}` / `{{!SELF}}` parse as a variable in the reserved
  `!` namespace; any other `!` name is `PatternError::UnknownBuiltin`.
  `bind_self(name)` substitutes it. `is_ground()` is false while it is
  present.
- `Pattern` records which byte ranges of its source a substitution filled in,
  carried through later substitutions and left out of equality, ordering,
  hashing and serialization. `Pattern::marked()` renders them in `<>`.
- `src/lib.rs`: `flatten_collective` binds each member with its own name
  before merging; `Agent::make_instance` binds the instance name;
  `Instance::materialize` binds it in the instance's own promises and wants;
  `rebuild()` binds every agent that stands on its own. No working agent holds
  an unsubstituted `{{!self}}`.
- `Tracker::marked_name(agent, name)` returns a name as that agent declares
  it, marked, or None when nothing in it was substituted. The display sites
  call it; the query rows in `src/query.rs` are unchanged and keep plain
  names.

## 3. Phase 2 — open variables

### Semantics

- An open variable is written `{{?x}}`. It is bound by whoever provides a
  matching behavior, not by the promise name. The explicit sigil keeps today's
  safety rule intact: a plain condition variable the promise name does not
  bind is still an error, so a typo is still caught.
- Allowed in conditions and in top-level wants.
- An open goal matches **ground provided behaviors only**: the names working
  agents declare, after self-substitution and instance bindings. It does not
  match parameterized provides.
- An open variable in a want is scoped to that want. A repeat inside one name
  must bind consistently. In a condition, its value may also be used by sibling
  conditions (§5); it never flows up into the promise name.
- Several matches: the goal is satisfied if any one match is satisfied. Each
  match is its own offer, shown under its ground name. This is today's
  multi-provider rule.
- No match: unsatisfied, shown under the pattern text itself.

### Determinism

The set an open goal is matched against is fixed once the working agents are
built. It is never matched against names some other resolution derived, so an
answer cannot depend on which resolutions ran first. Offers keep today's order:
agents by name, then declaration order. The match set is finite, so open goals
add no new termination risk.

### What ground-only matching gives up

```yaml
kind: Agent
name: A1
wants:
- name: "{{?svc}} grants {{!self}} read on R1"
---
kind: Agent
name: S1
provides:
- name: "{{!self}} grants {{agent}} read on R1"
```

After self-substitution this is a pattern on both sides. It does not match in
phase 2 and the want stays unsatisfied; it is the simplest phase 3 case. So
that this is not a silent surprise, an open goal that finds no match says in
its unsatisfied report that parameterized providers are not considered.

### Validation — reserving phase 3's meaning

These are errors, so that no contract acquires a meaning upward flow would
change:

- `{{?x}}` in a provides name
- one name used both as `{{x}}` in the promise name and `{{?x}}` in its
  conditions

Phase 2 also rejected a plain `{{doc}}` bound only by a sibling's `{{?doc}}`.
That shape now has a meaning (§5). The same open name in two conditions of one
promise is still an error, for the reason given there.

### SuperAgent reduction

An open condition passes through `reduce()` untouched and stays on the
collective as an external condition, matched during normal resolution, where
the collective itself may be the provider. This is the rule non-ground
conditions already follow (`parameterized-behaviors.md` §8.1).

### CLI

Command-line behavior arguments stay ground. `simulate` and `providers` reject
an open pattern. Open goals are reached only through contracts.

### Implementation

As built:

- `pattern.rs`: `{{?x}}` parses as a variable whose name keeps its `?`.
  `Pattern::has_open()` and `needs_binding()` tell an open goal from a name
  still waiting on a value. Matching an open goal against a concrete name is
  the existing `match_ground`.
- `Tracker::open_matches(goal)` lists the concrete promised names an open goal
  matches. `resolve_goal` resolves each as the concrete goal it is and gathers
  the offers under the open goal; `walk_for_cycles` descends the same way. The
  per-goal cache is keyed on the goal text as before.
- `Offer` carries the concrete behavior it keeps when it answers an open goal
  (`Offer::get_behavior()`), absent from serialization otherwise, so existing
  output is unchanged.
- `Tracker::non_ground_wants()` reports only wants holding a plain variable.
  `promise_graph.rs`, `trace.rs` and `diagram.rs` label an offer with the
  behavior it keeps; with no match the pattern text is the missing node.
  `src/query.rs` counts a promise matched by an open goal as used, lists an
  open want among a behavior's consumers, and names the matched promise in a
  failing chain's cause.
- CLI `simulate` and `providers` refuse an argument holding `{{`.
  `check-unsatisfied --summary` says, for an open goal with no match, that
  parameterized promises are not considered.

Not yet done:

- `<>` marking for values bound by matching a goal (phase 1 marking covers
  `{{!self}}` and Instance bindings only).
- `network_diagram.rs` still labels an open want's providers under the
  pattern text.
- The Inventory tab shows an unmatched open want as plainly "not provided",
  without the note the CLI gives.

## 4. Test plan

- **self** — substitution in provides names, conditions and wants; both
  spellings; unknown `!` name rejected; an Instance gets its own name; two
  instances of one Agent base yield distinct wants; a SuperAgent member keeps
  its own name folded and unfolded; the instanced-SuperAgent overlap asserted
  as the documented behavior.
- **marking** — `<>` present on declaration listings with the option on,
  absent with it off, never on edge labels; identity unaffected.
- **open** — the motivating chain end to end; two matching documents giving
  two offers, satisfied if either is; no match unsatisfied under the pattern
  text; an open want; a repeat within one name binding consistently; a
  parameterized provider not matched, with the report saying so; each
  reserved shape rejected by validation; an open condition surviving
  `reduce()`; the CLI rejecting an open argument.
- **shared values** — a value found by one condition used by a sibling, in
  either declaration order; one offer per candidate, satisfied and not; a
  candidate from one match never paired with another's sibling; nothing found
  leaving the siblings as written; values chained through three conditions; a
  loop through a shared value reported; conditions judged together in the
  provides listing; a waiting sibling not reported as a cause; the rejected
  shapes.
- **regression** — every existing test passes untouched; contracts with no
  `!` or `?` behave exactly as before.

## 5. Phase 3 — binding direction

Before this phase a variable's value always came from the promise name,
matched against a ground goal, and flowed down into the conditions. Phase 3
asks what happens when the value comes from the provider's side instead. Under
it a variable has exactly one binding occurrence, marked `?`; every plain
`{{x}}` is a use.

It has two parts. Sideways flow is built. Upward flow is still an open
problem.

### Sideways flow — a value shared between conditions

```yaml
provides:
- name: configured
  conditions:
    - "will support {{?thing}}"
    - "{{thing}} does stuff"
```

- **One binder.** Exactly one condition of a promise opens a name, as
  `{{?thing}}`. Sibling conditions that use it write plain `{{thing}}` and take
  the value the binder finds. Declaration order does not matter.
- **Candidates.** Each behavior the binder condition matches is one candidate
  value. Matching is phase 2's: only behaviors promised outright.
- **One offer per candidate.** The promise is read once per candidate: the
  binder condition answered by that one match, the siblings with the value
  filled in. Each reading is its own offer, satisfying when all its conditions
  hold. The promise is kept if any reading is. Every candidate is recorded,
  whether it holds or not.
- **Nothing found.** One unsatisfying offer, the binder condition under its
  pattern text and its siblings left as written. Only the binder is reported
  as the cause.
- **Chains.** A binder may itself use a sibling's value
  (`{{?replica}} runs in {{region}}` beside `{{?region}} hosts {{team}}`).
  Binders are taken in whatever order lets each have what it waits on, and the
  candidates multiply.
- **Not shared.** An open variable no sibling uses behaves as in phase 2: the
  condition gathers every match and any one satisfies it.

Why one binder, and not `{{?thing}}` in both conditions: two open occurrences
would each search on their own, and the pair would have to be intersected
afterwards. One finding and the rest using it says which condition the
candidates come from, and keeps a typo in the plain use an error.

Validation:

- a plain `{{x}}` in a condition must be named by the promise or opened as
  `{{?x}}` by a different condition of the same promise
- the same open name in two conditions is an error, with the fix in the
  message (write `{{x}}` in the second)
- binder conditions that wait on each other's values are an error

Limits:

- A promise's conditions get at most 256 readings; further candidates are
  dropped.
- Values do not flow between a promise's conditions and its name, or between
  wants.
- In a folded SuperAgent, conditions that share a value are not ground, so
  `reduce()` leaves them on the collective as external conditions.

As built:

- `Tracker::condition_readings` turns a promise's conditions, with the goal's
  bindings filled in, into one list of goals per reading. `resolve_goal` and
  `walk_for_cycles` descend through every reading. The binder's resolution in
  a reading keeps the pattern text as its name and carries only the chosen
  match's offers, each with the concrete behavior it keeps, so the graph, trace
  and sequence diagram label it as they do any open goal.
- `Tracker::resolve_conditions` gives the same readings to `src/query.rs`,
  which judges conditions that share a value together: a provides listing
  shows them by the candidate that lets the most of them hold.
- `leaf_causes` skips a sibling still waiting on a value when the binder is
  the real cause.

Not yet done:

- Nothing in the UI or CLI says which value an offer stands on, other than the
  names of its conditions.
- `Index::consumers` counts a sharing condition as a consumer of anything its
  text could match (`{{thing}} does stuff` uses every `… does stuff`), not
  only the candidates the binder found.

### Upward flow (open problem)

Not specified. It needs real unification and a termination argument to replace
the numeric caps.

**Provider's literal text answers the asker's open variable.** The example in
§3: `{{?svc}} grants A1 read on R1` meets `S1 grants {{agent}} read on R1`.
Each side fixes what the other left open — `svc = S1`, `agent = A1` — and A1's
want is satisfied by S1 under `S1 grants A1 read on R1`.

**A condition binds a variable in the promise's own name.**

```yaml
kind: Agent
name: A1
wants:
- name: "{{?region}} copy of R1 readable by {{!self}}"
---
kind: Agent
name: S1
provides:
- name: "{{region}} copy of R1 readable by {{agent}}"
  conditions:
    - "{{?region}} replica of R1 is healthy"
---
kind: Agent
name: Rep1
provides:
- name: "us-east replica of R1 is healthy"
---
kind: Agent
name: Rep2
provides:
- name: "eu-west replica of R1 is healthy"
```

A1 does not say which region, so S1's name cannot bind `region` from the goal.
The condition is the binder: it matches twice, each value flows up into the
name and back to A1, and A1's want gets two satisfied offers. Had A1 written a
ground want, the goal would fix `region` first and the condition would only
check it.

### Constraint on what is built

`?` remains the binder marker, and the shapes listed under phase 2 validation
stay errors until upward flow gives them meaning.
