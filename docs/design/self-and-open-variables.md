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
  `<A1> reads R1` and `A1 reads R1` are the same behavior. Marked text is
  never a key: node ids, selections, lookups and search all use the plain
  name.
- A UI toggle in the options bar ("Mark: Bound values"), above the tabs and
  so on every one of them, and a `--mark-bound` flag on CLI `provides`,
  `providers` and `unused`.
- Covers every substituted value:
  - `{{!self}}` and Instance `bindings:`, fixed when the working agents are
    built;
  - what a parameterized promise name took from the goal it keeps
    (`<A1> reads R1` against `{{agent}} reads R1`), and that value where it
    went on into the promise's conditions;
  - a value one condition found with `{{?x}}`, where a sibling uses it;
  - the part of a promised behavior an open goal's `{{?x}}` matched.
- A name is marked by who is looking at it. The agent that needs a behavior
  (a want, or a condition on its promise) marks what its side filled in; the
  agent that promises it marks what its own declaration names through a
  variable. So a want reads `Read Secret appsecret by <appname>` in its own
  agent's lists, while the provider that keeps it under
  `Read Secret {{secret}} by {{app}}` has `Read Secret <appsecret> by
  <appname>`.
- Where the two meet, both show. A graph edge, a sequence arrow, a trace
  card and a text row each stand for one behavior between a needing and a
  promising agent, and are marked with what either substituted. This reverses
  the earlier decision to keep marks off edge labels: the two declarations
  may be written differently, and the union is the reading that hides
  nothing. `<appname> has IAM role <approle>` is `appname`'s own
  `{{!self}}` together with the asker's `{{?approle}}`.
- Where a name belongs to no one agent — the Inventory's behavior list and a
  behavior's heading, a dependency loop in the cycle banner, a line ending
  in a scratch group — it is marked with what any agent substituted.
- A row shows what its own declaration substituted, and nothing a neighbor
  did with the same text. One agent can reach a name two ways — one promise
  writes a condition out, another finds it with `{{?x}}`; a want is written
  out, a condition names it through `{{!self}}` — and only the second of
  each is marked. So a list row is keyed by what it stands for: a want by
  (agent, want); a promise by (agent, the promise as written); a condition
  by (agent, the behavior its promise keeps, the condition). A trail is
  marked from the resolution of its own want, so a name another want
  reaches through a variable stays plain on this one, and each need row on
  a card keeps its own marks. With self promises folded a card takes over
  the needs of the agent's own promises it rested on, and can then list the
  same behavior from the same provider twice: `z is up`, the card's own
  condition written out, and `↩ <z> is up`, taken over from a folded promise
  that found it with `{{?v}}`. They stay two rows, each marked as its own
  condition has it.
- Where one row or card stands for several occurrences it shows the union
  of theirs, and only theirs: a row bundled over several agents; a row the
  inventory lists once for promises that read the same; a trace card placed
  once and reached by several needs on that trail, or bundled from several
  promises of one provider, whose needs for one target become one need
  row. The Overview's lines and nodes stand for an agent and a name, so
  there every promise of the agent with that name counts; its unsatisfied
  strip lists wants, and reads each as a want.
- A heading that names the want being looked at — the Detailed views'
  subtitle, the Trace title, the behavior select — shows it as its agent
  wrote it (`Tracker::marked_want`). File names stay plain.
- The Detailed text view names the behavior an open need was answered by on
  the offer's row, with the option on or off:
  `OPTION: appname — appname has IAM role approle`. Marked, that is where the
  match's own marks show (`<appname> has IAM role <approle>`). Bundled into
  one row per provider, the need reads `need → behavior kept`.
- CLI `agents`, `behavior`, `simulate` and `check-unsatisfied` carry no
  marking.

Where each view takes its label from, with the option on:

| View | Label |
|---|---|
| Overview graph | `fold::mark`: each promise on a line by `Marks::edge(source, target, behavior)`; a missing behavior's node by `Marks::needed`; the unsatisfied strip by `Marks::want(agent, want)`, a want as its agent wrote it |
| Detailed, Text | the resolution itself: heading `Resolution::marked_name` (from `Tracker::resolve_want`), each behavior row under it `marked_name_with_offers`, an offer's row under an open need `Resolution::marked_offer` |
| Detailed, Sequence | `DiagramOptions::mark_bound`: each arrow by the offer's and the needing side's spans together; the marks are written `#lt;` `#gt;` so Mermaid draws them rather than reading markup |
| Detailed, behavior select and frame subtitle | `Tracker::marked_want(component, want)` (`ViewOptions::want_label`) |
| Trace | wants list and frame title `Tracker::marked_want(agent, want)`; cards and their needs `Trail::labels_when`, from the spans the trail took from its own resolution, with no whole-tracker `Marks` built: the want card as its agent wrote it; a need row as the one condition it stands for filled it in, with what the answering promise names through a variable; a promise card by its own promise and every need reaching it on the trail; a missing card by the needs reaching it |
| Inventory, Agents | a promise `Marks::made(agent, promise, promise)`; its conditions `Marks::condition(agent, promise, …)` |
| Inventory, Behaviors | list and heading `Marks::any`; a provider's pattern and a consumer's promise `made_by(row agents, …)`; a provider's conditions `condition_by(row agents, the behavior, …)` |
| Inventory, Unsatisfied | the want `want_by(row agents, …)`; a cause `Marks::condition(site agent, site promise, cause)`; the promise it is a condition of `Marks::promise(site agent, …)` |
| Inventory, Unused | the promise `made_by(its agents, …)`; conditions `condition_by(row agents, the promise, …)` |
| CLI `provides`, `providers`, `unused` | as the Inventory's Agents, Behaviors and Unused rows |
| Cycle banner | `Marks::any` |

Known limits of the marking:

- Two promises of one agent that keep the same behavior and carry a
  condition of the same text are not told apart: the condition is keyed by
  the behavior kept, not by which of the two promises it sits on. If one
  reached it through a variable, both rows show the mark.
- An Unsatisfied cause names the promise it is a condition of by agent and
  behavior only, so that promise is marked with what any promise of that
  agent keeping the behavior substituted (`Marks::promise`, the one list
  row still read at the coarse grain).
- An Overview line or missing node is marked by agent and name
  (`Marks::edge`, `Marks::needed`): an agent that reaches a behavior both
  written out and through a variable has one line for it, and the line is
  marked. That is the union the line stands for, not a row of its own.
- A heading and the Trace's wants list take a want from the first
  declaration of it by that agent that substituted anything
  (`Tracker::marked_want`); the Inventory and the Overview strip take the
  union over the agent's declarations of that want (`Marks::want`). They
  differ only if one agent declares the same want text twice with
  different parts substituted.
- A trace card is placed once per agent and behavior, and its needs are
  hung the first time it is reached. A promise reached again further along
  the trail is a reference to that card and adds nothing to the card's own
  need rows.
- The spans are not serialized. A `Resolution` or `Trail` read back from
  JSON (the legacy `wpt` bindings) is unmarked.

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
  it — promise, condition or want — marked, or None when nothing in it was
  substituted. `Tracker::marked_want(agent, want)` looks at the agent's wants
  only and always returns text; it is what headings use.
- `Resolution` and `Offer` carry the spans resolution filled in
  (`src/resolve.rs`): a resolution, the parts of its name the needing side
  substituted; an offer, the parts of the behavior it keeps that its promise
  names through a variable, and under an open goal the parts the goal filled
  in or matched. They are set in `Tracker::resolve_goal` and
  `resolve_reading` from the substituted condition `Pattern`s. The spans take
  no part in equality, `Debug` or serialization (`#[serde(skip)]`: byte
  offsets are of no use to a reader of the JSON). A test reads them with
  `Resolution::marked_outline`.
- The per-goal cache in `resolve_goal` is keyed on goal text, and what it
  holds does not depend on how the goal was reached. The one thing that does
  — which parts of the goal's own name were substituted — is put on after the
  lookup (`as_reached_by`), never stored. `Tracker::resolve_want(agent,
  want)` does the same for a top-level want.
- `src/marks.rs`: `Marks::new(tracker)` reads every declaration and walks
  every resolution once, and keeps the spans at two grains. Coarse, by
  (agent that needs, name) and (agent that promises, name), for the
  Overview (`need`, `promise`, `edge`, `needed`, `any`). Fine, for a list
  row: (agent, want) → `want`; (agent, promise as written, behavior kept) →
  `made`; (agent, behavior the promise keeps, condition) → `condition`; each
  with a `_by` form for a row bundled over several agents. The graph and
  query rows are unchanged and keep plain names; a view asked to mark looks
  its label up by the same plain names.
- `src/trace.rs`: a `Trail` carries the spans of its own resolution
  (`Tracker::resolve_want`). What a need filled in is kept per need row, in
  step with each card's `needs` (a private `TrailSpans`), not by what the
  need is: two rows on one card can be the same behavior from the same
  provider and still differ in what was substituted. `without_own_promises()`
  moves a folded promise's rows, spans and all, to the card that takes them
  over; `bundled()` merges the spans of the rows it merges into one, and of
  no others. What a promise names through a variable is kept by (agent,
  behavior). `Trail::labels_when(mark)` builds the labels from those;
  `Trail::labels(marks)` is the same with `mark` taken from whether the
  tracker substitutes anything at all. Not serialized.
- Views that already walk a resolution — the text view, the sequence
  diagram — read the spans off it directly.

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

- `network_diagram.rs` (the legacy graph, drawn by no tab) still labels an open want's providers under the
  pattern text, and carries no marking.
- CLI `simulate`, `check-unsatisfied` have no `--mark-bound`.
- The Inventory tab shows an unmatched open want as plainly "not provided",
  without the note the CLI gives.

## 4. Test plan

- **self** — substitution in provides names, conditions and wants; both
  spellings; unknown `!` name rejected; an Instance gets its own name; two
  instances of one Agent base yield distinct wants; a SuperAgent member keeps
  its own name folded and unfolded; the instanced-SuperAgent overlap asserted
  as the documented behavior.
- **marking** — `<>` present with the option on and absent with it off, in
  each renderer: the marks table (`marks.rs`), the folded graph's lines and
  missing nodes, the sequence diagram (as entity codes), the trail's cards
  and needs, bundled and not, and the text rows; every kind of substitution
  on the IAM chain; a goal reached two ways marked each its own way despite
  the cache; identity, equality, `Debug` and serialization unaffected. A
  name one declaration writes out and another reaches through a variable is
  marked only for the second: two promises of one agent, a want and a
  condition, a pattern and an outright promise, two wants' trails; a card or
  row that merges several shows the union of exactly those. Headings mark
  the want only with the option on. A multi-byte name (`é1`) throughout.
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
  names of its conditions. With "Mark: Bound values" on, the value is at least
  set apart in them (`… to <approle>`).
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
