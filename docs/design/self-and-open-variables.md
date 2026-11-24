# Self-Reference and Declared Variables

**Status**: phase 1 implemented; phases 2 and 3 redefined (2026-11-24): the
`{{?x}}` open variable is replaced by a `forSome` declaration on the promise
and two-sided matching (§3, §5)
**Date**: 2026-11-19, revised 2026-11-24
**Scope**: core library, CLI, frontend
**Builds on**: `docs/design/parameterized-behaviors.md` (P0–P3 landed)

Three additions to parameterized behavior names, in the order they will land:

| Phase | Addition | State of the definition |
|---|---|---|
| 1 | `{{!self}}` — an agent refers to itself by a reserved variable | specified here, implemented |
| 2 | `forSome: [x]` — a variable a promise's conditions find a value for | specified here (§3) |
| 3 | Two-sided matching — a value found on the provider's side, parameterized or not | specified here (§5); a variable taking a variable plus text is deferred |

Phase 2 was first built as `{{?x}}`, an open variable written in the
condition itself. That syntax is removed (§3, "What `{{?x}}` was").

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
  forSome: [doc]
  conditions:
    - "{{doc}} shows usage of R1 by {{agent}}"
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
  - the value a declared (`forSome`) variable took, in every condition
    that uses it;
  - the part of a promised behavior a condition still holding a declared
    variable matched (a gathered row, §3).
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
  `{{!self}}` together with the value the asker's declared `approle` took.
- Where a name belongs to no one agent — the Inventory's behavior list and a
  behavior's heading, a dependency loop in the cycle banner, a line ending
  in a scratch group — it is marked with what any agent substituted.
- A row shows what its own declaration substituted, and nothing a neighbor
  did with the same text. One agent can reach a name two ways — one promise
  writes a condition out, another finds it through a declared variable; a
  want is written out, a condition names it through `{{!self}}` — and only the second of
  each is marked. So a list row is keyed by what it stands for: a want by
  (agent, want); a promise by (agent, the promise as written); a condition
  by (agent, the behavior its promise keeps, the condition). A trail is
  marked from the resolution of its own want, so a name another want
  reaches through a variable stays plain on this one, and each need row on
  a card keeps its own marks. With self promises folded a card takes over
  the needs of the agent's own promises it rested on, and can then list the
  same behavior from the same provider twice: `z is up`, the card's own
  condition written out, and `↩ <z> is up`, taken over from a folded promise
  that found it through a declared `v`. They stay two rows, each marked as its own
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
- The Detailed text view names the behavior a gathered row was answered by
  on the offer's row, with the option on or off:
  `OPTION: appname — appname has IAM role approle`. Marked, that is where the
  match's own marks show (`<appname> has IAM role <approle>`). Bundled into
  one row per provider, the need reads `need → behavior kept`.
- CLI `agents`, `behavior`, `simulate` and `check-unsatisfied` carry no
  marking.

Where each view takes its label from, with the option on:

| View | Label |
|---|---|
| Overview graph | `fold::mark`: each promise on a line by `Marks::edge(source, target, behavior)`; a missing behavior's node by `Marks::needed`; the unsatisfied strip by `Marks::want(agent, want)`, a want as its agent wrote it |
| Detailed, Text | the resolution itself: heading `Resolution::marked_name` (from `Tracker::resolve_want`), each behavior row under it `marked_name_with_offers`, an offer's row under a gathered row `Resolution::marked_offer` |
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
  names through a variable, and under a gathered row the parts the row
  filled in or matched. They are set in `Tracker::resolve_goal` and
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

## 3. Phase 2 — declared variables (`forSome`)

### Semantics

- A promise may declare variables its conditions find values for:

  ```yaml
  - name: "{{appname}} gateway is configured"
    forSome: [kc]
    conditions:
      - "{{appname}} deploys to {{kc}}"
      - "{{kc}} deploys {{appname}} gateway"
  ```

  The promise holds when, for some value of `kc`, every condition holds.
  `kc` is written plainly, `{{kc}}`, wherever it is used.
- A condition variable must be named by the promise name or listed in
  `forSome`. Anything else is a validation error, so a typo is still caught.
- Declared variables are local to the promise. They never flow into the
  promise name; the name's variables are filled in from the goal first.
- Wants are concrete once `{{!self}}` and Instance bindings are filled in. A
  want still holding a variable is not searched: it is reported by
  `Tracker::non_ground_wants` and stays unsatisfied, as it always was.
- How a value is found is §5: every condition that mentions a declared
  variable proposes values for it, from promises made outright or
  parameterized, and the proposals are pooled. Condition order never
  matters.

### Results

Resolution stays concrete. The search only proposes values; the existing
resolver, with its cache keyed on goal text, decides what is kept. The
search may over-propose — a value one condition proposes that another
condition does not hold for shows as an offer that is not kept — but it
never decides "satisfied" itself.

- **Shared** — a declared variable used by two or more conditions. The
  promise is read once per candidate value: one offer per candidate, its
  conditions the concrete behaviors with that value filled in. Candidates
  are never mixed: values found together (one match binding `a` and `b`)
  stay together. The promise is kept if any reading is. Every candidate is
  recorded, held or not.
- **Gathered** — a condition still holding a declared variable once the
  shared ones are filled in. It is one row, named by its pattern text,
  gathering every match as alternatives, any one of which satisfies it. Each
  offer names the concrete behavior it keeps (`Offer::get_behavior()`).
  Two independent variables on one promise give one offer with two gathered
  rows, not a cross product of offers.
- **Nothing found** for a shared variable: one offer that is not kept, its
  conditions left as written.

### Invalid versus unsatisfied

- **Invalid** (reported with the tracker-level checks, beside
  `non_ground_wants` and `dangling_instance_bases`; not a schema error): a
  declared variable whose only answer is a promise that holds for any value
  of it (§5), and nothing anywhere could name a value. For example:
  "`kc` can be anything: B's `{{appname}} deploys to {{kc}}` holds for any
  value and nothing names one".
- **Unsatisfied, not invalid**: something could name a value but not for
  this asker (C promises the gateway deploy only for `otherapp`).
- **Unsatisfied, not invalid**: nothing matches the condition at all.

### Validation

Per document (`src/validate.rs`):

- a condition variable that is neither in the promise name nor in `forSome`
- `forSome` on a promise without `conditions`
- a `forSome` name no condition uses
- a `forSome` name that is also a variable of the promise name
- a `forSome` name that is not a legal variable name
- a `forSome` name listed twice
- `forSome` on a want
- `{{?x}}` anywhere (see "What `{{?x}}` was" below)

### Determinism and termination

Offers keep today's order: agents by name, then declaration order. The
search uses resolution's two guards: an in-progress stack, compared with
variables renamed so that the same question under different variable names
is recognized, and the depth cap (`MAX_RESOLVE_DEPTH`, 64). Shared variables
keep the cap of 256 readings per promise (`MAX_CONDITION_READINGS`);
gathered rows have none.

### SuperAgent reduction

`reduce()` hoists the conditions of an inner promise a collective meets
itself into the outer promise. An inner promise's declared variables come
along: they are added to the outer promise's `forSome`, renamed where they
clash with a name already in use there. A condition still holding a variable
is never met internally; it stays on the collective as an external
condition, matched during normal resolution, where the collective itself may
be the provider (`parameterized-behaviors.md` §8.1).

### CLI

Command-line behavior arguments stay concrete. `simulate` and `providers`
refuse an argument holding `{{`.

### What `{{?x}}` was

The first form of phase 2 wrote the variable open in the condition,
`{{?doc}} shows usage`, and matched it only against behaviors promised
outright. Whether a contract resolved then depended on which condition
carried the `?`, and it never resolved when the answering promise was
parameterized. `{{?x}}` is removed everywhere, wants included. It is a
validation error whose message spells out the rewrite: declare
`forSome: [x]` on the promise and write `{{x}}`. There is no rewrite
command.

### Implementation

- `Behavior` (`src/components/behavior.rs`) has `for_some`, written
  `forSome` and placed before `conditions`, omitted when empty.
  `Behavior::instantiate` does not substitute a declared variable, so an
  Instance binding of the same name does not reach it. Wants are `Behavior`s
  too, which is why `forSome` on a want is a validation error rather than a
  parse error. The `deny_unknown_fields` message now lists `forSome` among
  the expected keys.
- `src/validate.rs`: `check_for_some` holds the declaring rules; the
  condition rule accepts a name of the promise or a declared one.
- `{{?x}}` is `PatternError::OpenVariable(x)` in `Pattern::parse`, whose
  message is the rewrite; as with any malformed name, `parse_lossy` keeps
  the text as literal, so a contract still loads and `check_items` reports
  it wherever it is (a want gets its own wording: there is no promise to
  declare on). `is_open`, `has_open`, `needs_binding`, `open_matches`, the
  binder readings (`condition_readings`, `ConditionGoal::Chosen`), the
  open-want cases in `Agent::has_ground_behavior` and `src/query.rs`, and
  the CLI's "parameterized promises are not considered" note are gone.
- `src/lib.rs`: `resolve_goal` and `walk_for_cycles` read a promise's
  conditions through `Tracker::readings(promise, conditions)`, which is
  `declared_readings`: the shared variables are
  those two or more conditions use; `candidate_values` pools what every
  condition mentioning a missing one proposes (`Tracker::search`), extends
  partial readings until complete, drops the ones that cannot be, caps them
  at `MAX_CONDITION_READINGS` and sorts them by value. Each reading's
  conditions are `ConditionGoal::Goal` once concrete, `Gathered` while still
  holding unshared declared variables, `Unbound` otherwise (nothing found,
  or a variable nobody declared).
- A gathered row is `Tracker::resolve_gathered`: the names
  `gathered_names` completes from the search, each resolved as the concrete
  goal it is, the offers kept under the pattern text with
  `Offer::answering(name)` and what the row's variables matched
  (`with_matched`). It is cached in `settled` under the pattern text, kept
  apart from concrete goals, and sits on the in-progress stack under that
  text, so a loop through it is reported by `dependency_cycles` with the
  pattern in it. `as_reached_by` re-marks any non-ground row with what its
  variables matched in each behavior kept.
- `Tracker::search` is the candidate search, `Tracker::propose(goal)` its
  public face, returning `Proposals { values, any }`: the value sets found,
  and the promises that held for any value (`HoldsForAny`). Its stack holds
  canonical goals; an answer not cut short is memoized by canonical text for
  the length of one call. Empty proposals are dropped.
- The invalid check is `Tracker::unnamable_variables()`, beside
  `non_ground_wants` and `dangling_instance_bases`: for each declared
  variable, the search runs on every condition using it as written (the
  promise name's variables open too, so a value named for any asker counts),
  and the variable is listed when nothing names a value and some promise
  holds for any. Its `Display` is the message. CLI `validate` reports it with
  the other tracker-level checks; that is the only place those are reported
  (the API and the editor run the per-document pass only).
- SuperAgent folding: `reduce_one` (`src/components/agent.rs`) declares an
  inner promise's `forSome` names on the outer promise when it hoists the
  inner conditions, renaming one already in use there (the outer name's
  variables, its own `forSome`, or one hoisted before) to `name_2`,
  `name_3`, ... with `Pattern::rename_vars`.
- Failure reporting: `src/query.rs` `leaf_causes` reports a condition row
  still holding a declared variable, with no offers, as
  `CauseKind::Unfound { variables, holds_for_any }`: the variables the
  search names no value for, and the promises that answered them while
  holding for any value (empty when nothing answered). `variable_causes`
  sorts out the rest of an offer's rows with no offers: a row sharing a
  variable with a sibling was never resolved (no reading completed); if
  each of its variables is named somewhere it is waiting, and not a cause
  while a sibling is `Unfound`, otherwise `Unfound` for all its variables (no
  set of values fills them together). A gathered row whose variables are
  named in full was resolved: every name it could stand for is promised by
  the promise that proposed it, so having no offer at all means resolution
  was cut short on the way, by a loop or the depth cap, and `Cycle` is
  what it is. Named in part only, it is `Unfound` for all its variables.
  CLI `check-unsatisfied
  --summary` prints `… finds no value for `kc`: only B's `…` answered,
  holding for any value`; the Inventory's Unsatisfied tab says `no value
  found for `kc`; only B's `…` answers, for any value`. `CauseKind` is no
  longer `Copy` (the new variant carries data), and `Cause` keeps its
  fields, so existing literals still build.
- `Tracker::resolve_conditions(conditions, declared)` takes the promise's
  declared names, for `src/query.rs`, whose `Resolver::conditions` now
  takes the promise.

Loops through a variable, as `dependency_cycles` sees them:

- A promise whose gathered row is answered by the promise itself (`a is
  ready` needing `{{x}} is ready`) is reported, the row under its pattern
  text: `["a is ready", "{{x}} is ready"]`.
- A promise reached again through a shared variable's reading (B's
  gateway through `kc = C`, when C's deploy needs the gateway) is reported
  by the concrete goals of the loop.
- A parameterized promise answering its own row (`{{n}} is ready` needing
  `{{x}} is ready`) names no value, since none of its conditions mention
  `n`: it holds for any value, and is reported by `unnamable_variables`
  rather than as a loop.
- A search that comes back to its own question with no value ever named
  (`{{a}} up` needs `{{a}} down` needs `{{a}} up`) stops and proposes
  nothing. It reaches no concrete goal, so like any parameterized loop
  (`parameterized-behaviors.md`) it is not reported until something grounds
  it; the condition shows as `Unfound`. Reporting it would mean the search
  returning the path it was cut short on, in pattern text; not done.

A consequence of pooling, kept as the rules give it:

- Two shared variables proposed by separate conditions are read in every
  combination proposed (a value for `a` from `{{a}} is ok`, one for `b` from
  `{{b}} is ok`), each its own offer.

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
- **declared variables** — the three-agent gateway example (§5) in both
  condition orders with the answering promise parameterized; a shared
  variable giving one offer per value, candidates never mixed between
  values; two independent single-condition variables giving one offer with
  two gathered rows; a gathered row with two matches satisfied if either is;
  a holds-for-any-only variable reported invalid while the two unsatisfied
  cases are not; a promise reaching itself through a variable terminating and
  reported as a loop; each declaring rule rejected; `{{?x}}` rejected with
  the rewrite in the message; a folded SuperAgent whose inner promise
  declares a variable; conditions judged together in the provides listing.
- **two-sided matching** — agrees with `match_ground` whenever one side is
  concrete; separate namespaces; a variable lining up with a variable;
  leftmost-shortest, never empty, a repeat agreeing.
- **regression** — every existing test that does not involve `{{?…}}` passes
  untouched; contracts with no variables behave exactly as before.

## 5. Phase 3 — two-sided matching

Before this phase a variable's value always came from the promise name,
matched against a concrete goal, and flowed down into the conditions. A
declared variable's value comes from the provider's side instead, and the
provider may itself be parameterized. Finding it is a candidate search in
front of the concrete resolver.

### The motivating case

```yaml
kind: Agent
name: A
wants:
  - name: "sampleapp gateway is configured"
provides:
  - name: "sampleapp CD deployjob is run"
---
kind: Agent
name: B
provides:
  - name: "{{appname}} gateway is configured"
    forSome: [kc]
    conditions:
      - "{{appname}} deploys to {{kc}}"
      - "{{kc}} deploys {{appname}} gateway"
  - name: "{{appname}} deploys to {{kc}}"
    conditions:
      - "{{appname}} CD deployjob is run"
---
kind: Agent
name: C
provides:
  - name: "{{!self}} deploys {{appname}} gateway"
```

A's want binds `appname = sampleapp`. Both conditions mention `kc`, so it is
shared. `{{kc}} deploys sampleapp gateway` meets C's `C deploys {{appname}}
gateway` and proposes `kc = C`. `sampleapp deploys to {{kc}}` meets B's
second promise with `kc` lined up against B's own `kc`; none of that
promise's conditions mention it, so it holds for any value and proposes
nothing. The pool is `{C}`, and the one reading, `sampleapp deploys to C`
and `C deploys sampleapp gateway`, is resolved as concrete goals: both hold,
so the want is satisfied. In either condition order.

### Matching

Two names are matched against each other with separate variable
namespaces: `{{kc}}` in a goal and `{{kc}}` in the promise it meets are
different variables.

- A variable on either side takes the other side's literal text, or lines up
  with exactly one of the other side's variables. `{{kc}} deploys sampleapp
  gateway` against `C deploys {{appname}} gateway` gives `kc = C` and
  `appname = sampleapp`.
- `Pattern::match_ground`'s conventions hold: leftmost-shortest, a binding is
  never empty, a repeated variable must agree (a variable lined up with a
  variable must line up with it again).
- With one side concrete this is exactly `match_ground`.
- **Wording must correspond** (the orchestrator's rule after the first
  round, not yet confirmed by the owner). Free-text variables make almost
  any two patterns unify: `{{replica}} is healthy` and `placed for {{team}}`
  share the instance `placed for is healthy`, which is not what an author
  means; left alone, a contract's own promises answered its conditions with
  such values, and the goals they spawned grew until the depth cap. So when
  both sides hold variables, each literal segment whose wording (its
  characters outside the pattern's substituted spans, `Pattern::bound`)
  holds a letter or digit must have at least one letter or digit of that
  wording matched against literal text of the other side, not taken by one
  of its variables. A segment that is only separators, or only a substituted
  value, is exempt. A match failing this is not a match, and the matcher goes
  on to the next way of matching. With either side concrete the rule does
  not apply.
  - Consequences: `{{replica}} is healthy` does not match `placed for
    {{team}}`; `sampleapp deploys to {{kc}}` does not match `{{appname}}
    gateway is configured`; `{{kc}} deploys {{appname}} gateway` does not
    match `{{appname}} deploys to {{kc}}`; `{{kc}} deploys sampleapp gateway`
    still matches `C deploys {{appname}} gateway`; `{{who}}/prod reads R1`
    still matches `db/{{env}} reads R1` when `db` came from `{{!self}}`.
  - Known limit: a hand-written value directly followed by a variable
    (`us-east {{thing}} is healthy`) is wording, so a goal variable cannot
    take `us-east` from it (`{{region}} replica is healthy` does not match).
    Writing the value through `{{!self}}` or an Instance binding makes it a
    value.
- Deferred: a variable taking the other side's variable plus text (`{{kc}}`
  against `C-{{zone}}`) simply does not match. A binding is held as a list of
  the other side's segments — today always one literal or one variable — so
  that can be added without changing the representation.

### The search

A goal here is a condition with what is known filled in, still holding
variables to find values for. For each promise (agents by name, then
declaration order) whose name matches the goal:

- A goal variable that took the promise's literal text has that text as a
  proposed value.
- A goal variable lined up with a promise variable: the search continues into
  that promise's conditions, with what the match filled in, to find values
  for it. Each condition mentioning it proposes; the proposals are pooled.
- If none of the promise's conditions mentions that variable, the promise
  holds for any value of it: it proposes nothing for it, and accepts whatever
  value something else proposes. This is what the invalid check (§3) looks
  for.

At the promise asking the question, every condition that mentions a shared
variable proposes candidates, and they are pooled. A proposal may name only
some of the variables; a reading is extended, with what it has filled in,
until each shared variable has a value, and a reading that cannot be
completed is dropped.

### Limits

- Values never flow into the promise name: a declared variable may not
  appear in it, and a name variable is never found by a condition.
- The search proposes from what working agents declare; it never reads what
  another resolution happened to derive, so an answer does not depend on what
  was resolved first.

### Implementation

- `pattern.rs`: `Pattern::unify(other)` is the two-sided match. It works a
  character at a time, a variable on the left settled before one on the
  right, so with one side concrete it makes exactly `match_ground`'s choices.
  It returns a `Unifier`: each side's variables with the other side's
  segments they took (`left_values`, `right_values`, `aligned`). A variable
  meeting a variable that already took text takes the same text; one meeting
  a variable already lined up with a different one does not match (`{{x}}
  and {{x}}` against `{{p}} and {{q}}`), the narrow reading of "exactly one
  of the other side's variables". The wording rule is the private
  `Wording` type in `pattern.rs`, consulted at two points in the matcher (a
  literal character matched against a literal character, and the end of a
  match); removing it leaves the rest of the matcher and the search as they
  are. `Pattern::canonical()` renames variables
  by order of appearance, for the search's in-progress stack.
