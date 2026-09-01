# Parameterized Behaviors and Instances

**Status**: P0–P3 landed, P4 outstanding
**Date**: 2026-08-31
**Scope**: core library, CLI, API, frontend
**Landed**: the tag removal that preceded this work (§8.2), then P0 `fb4b00f`,
P1 `86bcd8e`, P2 `8a847ff`, P3 `42d45da`. Sections corrected against the
implementation are marked *amended*.

Two changes that turn out to be the same idea at two levels. A **behavior**
becomes parameterized so one agent can promise the same thing to many agents on
different terms, with the value supplied by the consumer. A **component**
becomes parameterized so one definition can be copied into several real agents,
with the values supplied by the copy. The second is `kind: Instance`, promoted
out of `SuperAgent` where it currently hides.

## 1. Problem

A behavior's identity is its name, globally. `Tracker::resolve()` (`src/lib.rs:194`)
walks every working agent, string-compares `behavior_name`, and recurses into
`Behavior::get_conditions()` — also plain strings. Nothing about a promise is
scoped to who is receiving it.

This is fine until one agent needs to make the *same* promise to several other
agents on *different terms*. The motivating case:

```yaml
kind: Agent
name: host
provides:
  - name: process-execution
    conditions:
      - binary-installed
---
kind: Agent
name: p1
wants:
  - name: process-execution
---
kind: Agent
name: p2
wants:
  - name: process-execution
```

There is exactly one `binary-installed` in the universe, so the host's obligation
toward p1 and its obligation toward p2 collapse into a single node. If p1's
binary is installed and p2's is not, the model cannot say so.

### What is actually missing

Not exclusivity. Nothing is consumed today — every wanter of `b1` receives every
provider of `b1` as an offer, and `promise_graph::tests::test_multi_provider_want`
asserts exactly that. Non-exclusive fan-out already works.

What is missing is **per-relationship instantiation of the condition subtree**.
The graph makes the collapse visible: `PromiseGraphBuilder.edges` is keyed
`(source, target, behavior, kind)` (`src/promise_graph.rs:90`) and `merge_edge`
(`src/promise_graph.rs:123`) OR-merges `satisfied`, so the host's two condition
edges fuse into one edge that draws green if *either* process's binary is
installed.

### Prior art, since removed

`SuperAgentInstance.providesTag` / `conditionsTag` fed a `Behavior::make_instance`
that rewrote `b1` to `"b1 | i1p"` **and** rewrote that promise's conditions to
`"c1 | i1c"`. That was already a discriminator propagating from a promise into
its condition subtree — the exact mechanic needed here — but the discriminator
was baked into a string and unrecoverable, it was reachable only by wrapping
agents in a SuperAgent instance, and no contract ever used it. It has been
removed; see §8.2.

## 2. Options considered

| # | Option | Shape |
|---|---|---|
| 1 | Structured tags on `Behavior` | identity becomes `(name, tags)` |
| 2 | **Templated names** | `process-execution/{{process}}`, value supplied by the wanter |
| 3 | Explicit promisee scope | `provides: [{name: …, to: [p1, p2]}]` |
| 4 | Component instancing | declare host once, instantiate `host-for-p1`, `host-for-p2` |
| 5 | Binding documents | `kind: Binding` naming wanter, provider, arguments |
| 6 | Naming convention only | author `process-execution/p1` by hand on both sides |

### The deciding question: who supplies the discriminator?

The host contract must not have to know what processes exist. Run each option
against that requirement.

**Tags (1)** are constants. The host must write `tags: {process: p1}` and
`tags: {process: p2}` and be edited again for p3. The dependency arrow inverts
and the platform contract stops being reusable. A wildcard tag that binds and
flows into the conditions rescues it — but a wildcard that binds and propagates
*is a variable*, so that is option 2 with worse syntax.

**Promisee scope (3)** has the same enumeration problem, plus a deeper one: it
discriminates the *promise* per promisee but not the *conditions*. Fixing that
requires `conditions: ["binary-installed | {{promisee}}"]` — templating again,
with the variable hardcoded to be the promisee, which fails as soon as the
discriminator is not an agent (per-port, per-volume, per-database-name).

**Component instancing (4)** invents agents that do not exist. The graph grows
`host-for-p1` and `host-for-p2` nodes when there is one host, destroying the
"what is broken, on which machine" reading the viz exists to serve.

**Binding documents (5)** put the wiring outside the agents: an external
document asserting a relationship neither party declared is an imposition, not a
promise. Also O(N²) documents. Rejected on principle.

**Convention (6)** is the honest baseline. It is correct and free for two
processes and collapses at ten. The other options have to beat it.

**Templating (2)** is the only option where the provider writes the pattern once
and the *consumer* supplies the value.

### Why rejecting option 4 does not reject instances

§6 adopts instancing as a first-class kind, which looks like a contradiction.
It is not. The test is **whether the copies exist independently of who is
asking**:

- Two clusters genuinely exist. Instancing them is describing reality, and the
  graph should show two nodes.
- There is one host. `host-for-p1` is a fiction invented to work around a
  modelling gap, and the graph showing two hosts would be a lie.

Instancing discriminates *agents*; templating discriminates *promises*. Neither
subsumes the other, and §6 explains why agents specifically cannot be
materialized on demand the way promises can.

## 3. Decision

Adopt **parameterized behavior names**, under two restrictions that turn this
from a research project into a tractable change.

### Restriction A — wants are ground

Variables may appear only in `provides[].name` and `provides[].conditions[]`.
A `wants` entry containing a variable is a load-time error.

This is the whole argument for blast radius. `Tracker::resolve()` is always
called with a concrete goal, so:

- `Tracker::resolve(&str) -> Resolution` keeps its signature
- `Resolution.behavior_name` stays a `String`; so does `PromiseEdge.behavior`
- `who-provides <name>` and `simulate <behavior>` keep their arguments
- the REST API is unchanged
- `js_focus_promise_graph(component, behavior)` keeps working on concrete strings
- the `schemars` JSON schema is unchanged, because variables live *inside* the
  existing `name` string and no struct gains a field

Compare with structured tags, where behavior identity becomes a struct and every
place that treats a behavior name as a `String` — roughly 81 sites across 14
files including `diagram.rs`, `network_diagram.rs`, `resolve.rs`, two CLI
commands and three frontend components — has to change. In the end the whole of
that was avoided by one decision: `Behavior::get_name` keeps returning the
source text as a `&String`, so every caller that treated a name as a string
still does.

*Amended.* Where this restriction is **checked** moved during implementation —
one document cannot answer it. See §5.6.

### Restriction B — variables bind to atoms

A variable binds to a flat string. It never binds to another parameterized term.
Compound terms are a deliberate YAGNI; revisit only with a concrete use case.

*Amended.* **This does not by itself guarantee termination.** The first draft of
this section argued that it kept the system in Datalog rather than Prolog: the
derivable goals being substitutions of already-present atoms into
already-present patterns, a finite set. Implementation showed the hole. A
condition may name something *longer* than the promise carrying it, and string
concatenation is term construction under another name. `{{x}}` conditional on
`{{x}}x` derives `a`, then `ax`, then `axx`, without end — and because the goal
never repeats, the cycle guard cannot see it.

Two caps close it, and both are in the code:

| Cap | What it bounds |
|---|---|
| `MAX_RESOLVE_DEPTH` (64) | how deep a chain of conditions may go before resolution stops descending |
| `MAX_REDUCE_STEPS` (1000) | how many expansions one promise may go through in `reduce()` before the remainder passes through untouched |

The cleaner fix is a rule that a condition may not extend the term it belongs
to, which would restore a real finiteness argument in place of a numeric bound.
The caps are what ships, with a test asserting an ever-growing goal terminates.

### Explicit non-goal: capacity

"This host can only run four processes" is not solved by templates *or* tags. It
needs counts on edges and allocatable offers — a different resolution semantics.
Out of scope; it must not influence the syntax chosen here.

## 4. Syntax and semantics

### Surface syntax

```yaml
kind: Agent
name: host
provides:
  - name: "process-execution/{{process}}"
    conditions:
      - "binary-installed/{{process}}"
---
kind: Agent
name: p1
wants:
  - name: "process-execution/p1"
---
kind: Agent
name: packaging
provides:
  - name: "binary-installed/p1"
```

`resolve("process-execution/p1")` matches the host's pattern with
`process = p1`, substitutes into the condition to get `binary-installed/p1`, and
recurses. p2 gets its own subtree, satisfied or not on its own merits. The `/`
is convention, not syntax — any literal text may surround a variable.

### Term grammar

A **pattern** is a sequence of literal segments and variable segments.

- A variable is `{{` `identifier` `}}`, identifier matching `[A-Za-z0-9_-]+`.
- A pattern with zero variables is **ground** and behaves exactly as today.
- Two variables may not be adjacent. `{{a}}{{b}}` is a load-time error, because
  it has no deterministic reading.
- Matching is **leftmost-shortest**. `{{a}}-{{b}}` against `x-y-z` yields
  `a = x`, `b = y-z`.
- A variable may repeat within a pattern. `{{p}}-{{p}}` matches `x-x` but not
  `x-y`: repeated occurrences must bind consistently.
- A variable binds to a non-empty string. `run/{{p}}` does not match `run/`.

### Safety rule

Every variable appearing in a `conditions[]` entry must also appear in that
behavior's `name`. Otherwise the condition still holds an unbound variable at
resolution time and cannot be resolved. Load-time error.

This is the standard Datalog range-restriction rule and it is what guarantees
that every recursive goal is ground.

### Multiple matching providers

`process-execution/{{p}}` and `process-execution/special` both match the goal
`process-execution/special`. **Both become offers.** No specificity ordering, no
cut. This is consistent with the existing multi-provider semantics, where a
wanter simply sees every agent that can help.

### Ground goals only

`Tracker::resolve` accepts a ground goal, matches provider patterns against it,
and recurses on ground conditions. There is no general unification: one side is
always concrete. This is materially simpler to implement and to reason about
than two-sided unification, and Restriction A is what buys it.

## 5. Implementation — behaviors

### 5.1 New: `src/components/pattern.rs`

```rust
pub enum Segment { Literal(String), Var(String) }

pub struct Pattern { segments: Vec<Segment> }

pub type Bindings = BTreeMap<String, String>;

impl Pattern {
    pub fn parse(s: &str) -> Result<Pattern, PatternError>;
    /// Parse, treating anything malformed as one literal segment, so a bad
    /// name loads and behaves as it did before patterns existed. Validation
    /// re-parses with `parse` and reports it there.
    pub fn parse_lossy(s: &str) -> Pattern;
    pub fn is_ground(&self) -> bool;
    pub fn vars(&self) -> BTreeSet<&str>;
    /// Match a concrete name, leftmost-shortest, consistent repeats.
    pub fn match_ground(&self, name: &str) -> Option<Bindings>;
    /// Substitute. Unbound variables are left in place, yielding a pattern
    /// that is still parameterized — see partial binding in §6.
    pub fn substitute(&self, b: &Bindings) -> Pattern;
    pub fn as_source(&self) -> String; // round-trips to the original text
}
```

Pure, no dependencies on the rest of the crate, fully unit-testable on its own.
This is the first and largest piece of new code, and it lands before anything is
wired up. It is also what `kind: Instance` needs for `bindings`, so P0 is
shared by both halves of this design.

**Parse, do not interpolate.** `Behavior` stores the parsed `Pattern`, not the
raw string. Serialization renders back through `as_source()`. Keeping base name
and arguments recoverable in memory is what buys the UI affordances that
structured tags were supposed to provide — group edges by base behavior, collapse
`process-execution/*` into one bundle at low zoom, filter by argument value.

### 5.2 `src/components/behavior.rs`

- `name: String` becomes `name: Pattern`; same for each entry in `conditions`.
- `Serialize` / `Deserialize` go through `as_source()` / `parse()`, so the YAML
  surface and the JSON schema are unchanged.
- `Hash`, `Ord`, `PartialEq` derive over the parsed form; two behaviors written
  identically remain equal.
- Add `is_ground()`, delegating to the name pattern.

### 5.3 `src/components/agent.rs`

`get_provides(&self, behavior_name: &str) -> Option<HashSet<Behavior>>`
(`src/components/agent.rs:166`) becomes:

```rust
pub fn get_matching_provides(&self, goal: &str) -> Vec<(Behavior, Bindings)>
```

`Vec`, not `HashSet`: two matches of the same behavior can carry different
bindings, and declaration order is deterministic where `HashSet` iteration order
is not. The current `HashSet` return is a latent source of nondeterministic
offer ordering when one agent declares the same behavior twice; switching to
declaration order fixes it.

`Agent::make_instance` (`src/components/agent.rs:255`) gains a `Bindings`
argument and moves its logic behind `Instance::materialize` — see §6.

### 5.4 `src/lib.rs` — resolution

```rust
for (behavior, bindings) in variant_agent.get_matching_provides(goal) {
    if behavior.is_unconditional() { /* satisfying offer, as today */ }
    let resolved = behavior.conditions()
        .map(|c| c.substitute(&bindings))   // fully ground under the safety rule
        .map(|c| self.resolve(&c))
        .collect();
    /* satisfied / unsatisfied split, as today */
}
```

**Cycle guard, now mandatory.** `resolve()` today has no visited set: two
mutually conditional agents already recurse forever. Templating does not create
that bug but will make it easy to hit. Carry a goal stack; re-entering an
in-progress goal stops the descent.

*Amended.* That re-entry yields a resolution with **no offers at all** rather
than an offer marked as cyclic. Marking it would mean a new field on `Offer`,
changing its serialization and reaching the frontend — scope this phase did not
need. A promise that depends on itself keeps nothing either way, and a test
asserts the exact shape.

**Memoize completed ground goals.** `promise_graph()` calls `resolve()` once per
want and re-derives the same subtrees repeatedly. A per-query cache keyed on the
ground goal string is a straight win and becomes more valuable once one pattern
serves many goals. A goal whose subtree was cut short by either guard is *not*
cached: that answer depends on the path it was reached by.

### 5.5 `src/lib.rs` — enumeration split

The enumeration APIs become ill-defined once a declaration is not a name. Split
them along the ground/pattern line:

| API | Meaning |
|---|---|
| `has_behavior(name)` | *unchanged*: is `name` declared literally, as written |
| `has_ground_behavior(name)` | is this concrete name answerable — matches a ground declaration **or** a pattern |
| `get_working_behaviors()` | *unchanged*: ground declarations only |
| `get_behavior_patterns()` | the non-ground declarations |
| `get_agent_provides(agent)` | ground provides; add `get_agent_provide_patterns(agent)` |

`has_behavior` has three real callers, all of them guards placed *before* a
resolve: `frontend/src/components/contract_graph.rs:210` and
`frontend/src/components/contract_text.rs:271,365`. Under templating a user who
types `process-execution/p1` when only the pattern exists would be turned away by
`has_behavior`. **All three switch to `has_ground_behavior`.** This is the single
most important consequence of the split, and it is why the split is not
cosmetic. `wpt/src/lib.rs:87` re-exports `has_behavior` to the legacy WASM
bindings and should gain the new method alongside it.

### 5.6 Validation

*Amended in two ways: where the pass lives, and which checks it can make.*

A semantic pass runs after parsing. It lives in the **library**, at
`src/validate.rs`, not in `api/src/validation.rs` as first planned: the frontend
keeps its own separate copy of `validate_contract`, so putting the rules in the
API module would have meant maintaining them twice. The CLI `validate`, the API
`PUT` and the editor all call the one implementation.

What one document can be judged on:

- malformed or unbalanced `{{ }}`
- empty or invalid variable identifier
- adjacent variables
- a variable in `conditions[]` not bound by the behavior's `name` (safety rule)
- an `Instance` binding whose value contains `{{`, which would not round-trip
  through the pattern's own source

What one document **cannot** be judged on, because the answer depends on
documents it cannot see:

| Check | Why it moved | Where it lives |
|---|---|---|
| a want must be ground (Restriction A) | a want may legitimately carry a variable that an instance's `bindings` fill in, and this pass cannot know whether anything instantiates the document in front of it | `Tracker::non_ground_wants()` |
| an `Instance`'s `base` must resolve | a base may perfectly well be declared in another file | `Tracker::dangling_instance_bases()` |

Both are reported by `cli validate` once every file is read, and the frontend
can call them too since it builds one tracker over all contracts. This is a
genuine correction: the original placement would have produced false positives
on any contract whose base lived elsewhere.

The document-level errors are semantic, not syntactic, so they cannot ride on
serde_yaml's line/column marks the way `Item`'s hand-written deserializer
carefully preserves them. Each carries the owning document's name plus the
behavior name; the edit modal renders them where it renders parse failures.

## 6. Instances as a first-class kind

### Why promote it

A SuperAgent is a collection of Agents. That instances currently live *inside*
it is a byproduct of there being no Instance kind, not a statement about what an
instance is. Once bindings exist, an instance is the same idea as a
parameterized behavior applied one level up: the base declares patterns, the
copy supplies values.

Instancing and templating discriminate different things, and neither subsumes
the other:

- **templating** discriminates *promises* — one agent, one pattern, many
  concrete promises, value supplied by the consumer
- **instancing** discriminates *agents* — one definition, many agent copies,
  values supplied by the copy

Agents cannot be materialized on demand the way promises can. `promise_graph()`
walks `get_working_agent_names()` to find wants, and a wanter's wants must be
ground under Restriction A, so the set of agents needs an explicit enumeration
up front. A host with no processes still belongs on the canvas; a promise with
no promisee correctly does not. `kind: Instance` *is* that enumeration.

Without bindings, instancing is half a feature: two instances of one collective
are indistinguishable to `resolve()`, so you get N nodes on the graph and no way
to address any of them. That is the state of the code today, and it is why
`bindings` is not deferred.

### v1 document shape

```yaml
kind: Instance
name: prod-cluster
comment: production kubernetes
base: SuperAgent/kube-cluster     # or Agent/host, or a bare name when unambiguous
bindings:
  env: prod
  region: us-east
provides:                          # instance-only additions
  - name: prod-only-audit-hook
wants:
  - name: pagerduty
```

`base` reuses the `Kind/name` convention that `Item::get_name()` already
produces, so the parser is a split on the first `/`. A bare name is accepted
when it is unambiguous across both namespaces; ambiguity is a validation error,
never a silent pick.

### Rust

```rust
// src/components/instance.rs
pub struct Instance {
    name: String,
    comment: String,
    base: BaseRef,                     // { kind: Option<Kind>, name: String }
    bindings: BTreeMap<String, String>,
    provides: Vec<Behavior>,
    wants: Vec<Behavior>,
}

impl Instance {
    /// Ground the base's promises through `bindings`, then layer on this
    /// instance's own provides and wants.
    pub fn materialize(&self, base: &Agent) -> Agent;
}
```

`SuperAgentInstance`, `SuperAgent::instances`, `with_instance`, `get_instances`
and `get_instance_names` all delete. `SuperAgent` becomes
`{ name, comment, agents }` — a collection of Agents and nothing else.

### rebuild()

```
1. index agents / superagents / instances by name
2. suppressed = { agents contained in any superagent }
              ∪ { any base named by an instance }        # a base is a template
3. each superagent not suppressed  -> merge members, reduce(), emit
4. each instance                   -> resolve base (superagent bases reduce first),
                                      substitute bindings, add own provides/wants, emit
5. each agent not suppressed       -> emit
```

Still a single pass over a precomputed suppression set, with no topological
sort. That is only true because of the two v1 exclusions below, and it is the
main thing keeping this change small.

### v1 decisions

**A base referenced by an instance does not appear as a node.** It is a
template. This preserves exactly what a SuperAgent with instances does today,
generalizes it to Agents, and needs no new field and no migration. The explicit
alternative is `abstract: true` on the base — more predictable, one more field,
and a migration; deferred unless the implicit rule surprises someone in practice.

**Partial binding is allowed.** After an instance substitutes what it knows,
leftover variables stay as patterns for the consumer to bind:
`kube-api/{{env}}/{{tenant}}` where the instance fixes `env` and the wanter
fixes `tenant`. The one constraint is Restriction A, enforced *after* instance
binding: a `wants` entry must be ground once the instance has substituted.
Provides may stay parameterized.

**A dangling `base` does not make `rebuild()` fallible.** `rebuild()` is called
from `add_agent` / `add_superagent` / `add_item`, all infallible and all called
from the CLI, `wpt`, and the frontend; making it return `Result` is a bad ripple
for what is a load-time authoring error. The instance materializes with only its
own provides and wants, and `Tracker::dangling_instance_bases()` reports it —
not the per-document pass, for the reason given in §5.6.

**Instance names are ground in v1.** `name` is a plain string, not a pattern.
Pattern-named instances arrive with `matrix:` in v2.

### Effort

| File | Change |
|---|---|
| `src/components/instance.rs` | new, ~120 lines plus tests |
| `src/components/superagent.rs` | net **deletion**, ~40 lines |
| `src/components/item.rs` | 3 lines: `Kind` variant, dispatch arm, `get_name` arm |
| `src/lib.rs` | `available_instances`, `add_instance`, `add_item` arm, `rebuild()` ~50 → ~70 lines |
| `api/src/validation.rs` | one match arm, base-reference check, message text |
| `frontend/…/contract_card.rs` | one arm, so Instances do not fall into the agent bucket via `_` |
| `frontend/utils/validation.rs`, `wpt/src/lib.rs` | message text; one `PTComponent` variant |
| `samples/kube.yaml` | migrate the `sa1` instance block |

Roughly a day. It is this small because `Item`'s hand-written deserializer was
built for adding kinds, only two exhaustive matches on `Item` variants exist
outside `item.rs` (`src/lib.rs:62` and `api/src/validation.rs:34`), and the
frontend never matches the enum at all — `contract_card.rs:45` switches on the
raw YAML `kind` string and already has a `_` catch-all.

**Sequencing note.** An Instance with no `bindings` is a pure rename-plus-
additions and needs no `Pattern`, so this phase can land before, after, or
alongside P0–P2. It is a second breaking format change (`instances:` under
`SuperAgent` goes away), which is cheap for the same reason the tags were:
nothing outside tests uses it, and `samples/` is not tracked. Batching both
breaks into one release beats spacing them out.

## 7. Realized vs offered

Both views are wanted, and they answer different questions.

| View | Source | An unwanted pattern | Question it answers |
|---|---|---|---|
| **Realized** | resolution of actual wants | invisible | what is broken |
| **Offered** | `provides`, read directly | listed | what is on the table |

**Realized** is what `promise_graph()` produces today, unchanged: ground
instances reached by resolving actual wants. A pattern that nothing wants
contributes nothing, and for any simulation or expansion an unwanted template is
correctly an invisible promise.

**Offered** is the catalog: what an agent puts on the table, patterns included,
read straight off `provides` without resolving anything. It is the only place a
never-wanted pattern is visible — and, by the same logic, the natural place to
show an instance base, which §6 suppresses from the realized graph.

The two must not be merged into one graph, because a pattern has no promisee and
therefore no edge. Proposed shape:

- **library** — `src/catalog.rs`, `pub fn offered(tracker: &Tracker) -> OfferedData`,
  returning per-agent offers as `{ pattern, vars, conditions, is_ground }`,
  including suppressed instance bases. Structured enough to render either as a
  table or as a graph overlay.
- **CLI** — `behaviors` and `who-provides` print patterns alongside ground names,
  marked; add `--patterns-only` / `--ground-only` filters.
- **UI** — a second toggle in `DisplayOptions`
  (`frontend/src/components/display_options.rs:8`) next to the existing
  self-promises button. When on, each agent gains dashed template edges to
  `template:<pattern>` ghost nodes, visually distinct from the `missing:` ghosts
  already in `PromiseNodeKind`.

## 8. Known wrinkles

### 8.1 SuperAgent reduction

`Agent::reduce()` expands a condition when it is internally provided. With
patterns, "internally provided" can mean a pattern condition against a pattern
provider — two non-ground sides, which is exactly the general unification that
Restriction A was chosen to avoid.

**Phase rule**: `reduce()` expands only when the condition is ground. A
non-ground condition inside a SuperAgent passes through unreduced and surfaces
on the flattened agent as-is. Document it; test it. The eventual fix is one-sided
matching where the internal provider's pattern must be at least as general as
the condition, and it should wait for a case that needs it.

### 8.2 SuperAgent tags: removed, not subsumed

An earlier draft of this design proposed keeping `providesTag`/`conditionsTag`
and adding `bindings` alongside them. An audit found the mechanism entirely
unused: the only contract anywhere with an `instances:` block is
`samples/kube.yaml` (itself untracked), both tags there were `""`, and `""`
short-circuited `Behavior::make_instance` to an exact no-op. Every non-empty tag
value in the repo lived in `#[cfg(test)]`. `git log -S providesTag` over data
files returned nothing, ever. They were also the only two fields on
`SuperAgentInstance` without `#[serde(default)]`, i.e. *required* — which is why
`kube.yaml` carried them as boilerplate.

**They have been removed.** All 141 workspace tests pass, and CLI output across
every sample is byte-identical before and after. Seven mutations of the
remaining instancing paths were all caught by tests, including one gap closed in
the process (instance-specific `wants` were constructed by a test and never
asserted).

The one breaking change: `SuperAgentInstance` carries `deny_unknown_fields`, so
a contract still holding the old keys is now a hard parse error rather than
being ignored —

```
error: old-format.yaml:7:5: instances[0]: unknown field `providesTag`,
       expected one of `name`, `comment`, `provides`, `wants`
```

which can only affect a contract in browser localStorage or an API storage dir.

`bindings` on `kind: Instance` (§6) is the replacement, and it is strictly more
expressive: the tag blanket-suffixed every promise of an instance, where a
binding substitutes named variables the base author chose.

### 8.3 Ambiguous authoring

`{{` is not currently reserved. A behavior legitimately named `a{{b` becomes a
parse error. Given that behavior names in the samples are shell-ish identifiers
this is judged acceptable; the validation message should say so plainly rather
than reporting a generic syntax failure.

## 9. Phasing

Each phase is independently shippable and independently testable.

| | Phase | Contents | |
|---|---|---|---|
| **P0** | Pattern type | `pattern.rs` with parse, match, substitute, round-trip. No wiring. Property tests on match/substitute round-tripping. Shared by P1 and P3. | `fb4b00f` |
| **P1** | Resolution | `Behavior` holds patterns; `get_matching_provides`; `resolve` substitutes; cycle guard; memoization. Library only. CLI `simulate` and `check-unsatisfied` light up with no changes of their own. Every existing resolve test must pass untouched — ground patterns are the identity case. | `86bcd8e` |
| **P2** | Enumeration and validation | The `has_ground_behavior` / `get_behavior_patterns` split, the three frontend guard call sites, the semantic validation pass and its error surface in CLI `validate` and the edit modal. | `8a847ff` |
| **P3** | `kind: Instance` | §6 in full. Independent of P0–P2 except that `bindings` needs `Pattern::substitute`; can land first if the format break is wanted early. | `42d45da` |
| **P4** | Offered view | `catalog.rs`, CLI flags, the `DisplayOptions` toggle, template ghost nodes, suppressed instance bases. | outstanding |

Each landed phase was verified twice: green in the working tree, then again in
a clean checkout of the commit itself, so the history bisects. Test counts run
141 (before) → 162 → 172 → 180 → 194.

## 10. Test plan

- **`pattern.rs`** — parse round-trip; ground patterns; leftmost-shortest with
  multiple variables; repeated-variable consistency; empty-binding rejection;
  adjacent-variable rejection; every validation error variant.
- **resolve** — the motivating host/p1/p2 case, asserting p1 satisfied and p2
  not; pattern and ground provider both offering for one goal; a template with
  no wanter producing no edges; a cyclic condition terminating.
- **instances** — a bound instance's promises are ground and distinct per
  instance; two instances of one base are addressable separately (the assertion
  that today's code cannot satisfy); partial binding leaving a provides pattern
  open; a non-ground `wants` after binding rejected; a suppressed base absent
  from the working set; a dangling `base` reported by validation, not panicking.
- **superagent** — a non-ground condition passing through `reduce()`
  unexpanded.
- **graph** — realized graph unchanged for all-ground contracts (the existing
  suite is the regression net); offered view listing an unwanted pattern and a
  suppressed base.
- **validation** — each error variant reaches the API and CLI with the owning
  document and behavior named.

## 11. Deferred

Named here so they stay out of v1 scope without being forgotten.

**Instance of an Instance.** Bindings are ground substitutions, so a fully bound
instance has nothing left to bind; the case only matters under partial binding.
Excluding it is what lets `rebuild()` stay a single pass. Load error with a
clear message in v1.

**A SuperAgent containing Instances.** "prod environment = prod-cluster +
prod-db" is a real thing to want. It is the one feature that forces genuine
dependency ordering in `rebuild()`, which is why it is the natural second step
rather than part of this. In v1 `agents:` continues to reference Agents only.

**`matrix:` — many instances from one document.**

```yaml
kind: Instance
name: "cluster-{{env}}"
base: SuperAgent/kube-cluster
matrix:
  - { env: prod }
  - { env: staging }
```

One document expands to N internal instances at load time, the name rendered per
row through `Pattern::substitute`, with the safety rule requiring every matrix
key to appear in `name`. Purely additive on top of §6 — it changes nothing above
it, which is why it does not gate the refactor.

**`abstract: true` on a base**, replacing the implicit suppression rule in §6 if
that rule proves surprising.

**Compound terms** — lifting Restriction B so a variable can bind to another
parameterized term. Deliberate YAGNI; revisit only with a concrete use case, and
only with a termination argument to replace the Datalog one.

**Capacity and cardinality** — counts on edges, allocatable offers, "this host
runs at most four processes". A different resolution semantics, and explicitly
not allowed to influence the syntax chosen here.

**One-sided generality matching in `reduce()`** — see §8.1.

## 12. Alternative if this proves too heavy

Option 6, naming convention, remains available and costs nothing. If real
contracts only ever need two or three discriminated relationships, hand-written
`process-execution/p1` on both sides is a correct model and this whole design
can be deferred. Templating earns its keep at the point where a platform
contract is authored once and consumed by an open-ended set of agents — which is
the motivating case, so it is judged warranted now.
