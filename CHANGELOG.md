# Changelog

All notable changes to this project are documented in this file.

## Unreleased

A rebuild around one idea: a step is data. The runner walks the steps and
hands each action to the host, and the host answers. The public surface
drops from 66 items, 7 traits, and 3 modules to 19 items and 1 trait.

### Added

- `Step<A, K>`: one step of a sequence, as data. `Act` hands the host's
  own action type `A` to the host. `When`, `Branch`, `Call`, `Goto`, and
  `Return` are control flow.
- `Library<A, K>`: named sequences. `SequenceRef` is a sequence name, so a
  file can refer to a sequence before it defines it.
- `Library::validate`: steps that name a missing sequence, rules with an
  authoring warning, and steps that never run.
- `Runner::advance` and `Runner::resume`, with `Status`, `Answer`,
  `Abort`, `Busy`, and `Limits`. The wait for an action is the gap between
  the two calls.
- `Has<K>`: one trait answers every rule leaf. A host type can store some
  keys and compute others.
- `Unlocks::view` and `Unlocks::status`, with `NodeView` and `NodeStatus`:
  each node's status, rank, and edges for a tree view, in one call.
- With `serde`, `Step`, `Library`, `SequenceRef`, and `Runner` load and
  save. A runner saves in the middle of a chain, and `waiting_on` returns
  the action to show again after a load.

### Changed

- `Requirement::satisfies`, `Requirement::evaluate`, and the `Unlocks`
  queries take any `Has` holder. `Unlocks::take` needs a holder that is
  also `Extend`, so the grants land where `has` reads them.
- A rank is the earliest round in which the entity can take a node. A
  second source of a key no longer makes a false cycle.
- `Unlocks::dependencies` returns only the sole source of a required key.
  Several sources of one key are optional dependencies, and no node is
  both.
- `Requirement::optional` leaves out keys that the rule also requires.
- `Evaluation::missing` names each key once.
- Loading a file fails on an unknown field and on a name that appears
  twice.
- `Requirement::warning` is an inherent method. `UnlockWarning` is
  `non_exhaustive`.

### Removed

- The closure sequence API: `Sequence`, `Iter`, `ValidationWarning`, the
  old `Library` and `Runner`, `RunnerConfig`, `RunnerEvent`, `Outcome`,
  `AbortReason`, `SkipReason`, `StartError`, `ChainGuard`, the `Step`,
  `StepRun`, `IntoProgress`, `Condition`, `Effect`, `SequenceSource`, and
  `SequenceFacts` traits, `Progress`, `Flow`, `StepFacts`, `Context`,
  `TypeMap`, `ChainFlags`, `Completion`, `QueryCtx`, `EffectCtx`,
  `FlowModel`, `RailNode`, `RailShape`, and the `steps`, `conditions`, and
  `effects` modules.
- `CapabilitySet`. Use a `BTreeSet` or any `Has` holder.
- `Requirement::Flag`, `Requirement::Named`, `Evaluation::Opaque`,
  `Nothing`, `Rule`, `satisfies_in`, `unknown_checks`, and `Checks`.
- `Unlocks::rank` and `Unlocks::missing`.
- `UnlockWarning::Cycle`. `UnlockWarning::Unreachable` replaces it, and it
  also covers a node behind a loop.
- The `std` feature. The runner runs no host code, so it needs no panic
  guard.

### Migration

| 0.3 | 0.4 |
|---|---|
| `steps::run("Greet", \|ctx\| ...)` | an `Action` variant, `Step::act(Action::Greet)`, and a `match` arm in the host |
| A step that returns a `Completion` | an action that the host answers later |
| `Progress::Call`, `Goto`, or `Return` from a closure | `Answer::call`, `Answer::goto`, or `Answer::Return` |
| `steps::Branch { condition, if_true, if_false }` | `Step::Branch { rule, if_true, if_false }` |
| `steps::when(condition, step)` | `Step::when(rule, step)` |
| `steps::goto`, `steps::stop`, `steps::Call`, `steps::Return` | `Step::goto`, `Step::stop`, `Step::call`, `Step::Return` |
| `Rule::flag("x")` and `SetFlag` | a key that the host's `Has` stores, and an action that stores it |
| `Requirement::named("x")` and `Checks` | a key that the host's `Has` computes |
| `conditions::check` | a computed key |
| `effects::grant` and `effects::revoke` | actions that the host performs |
| `CapabilitySet<K>` | `BTreeSet<K>` |
| `tree.rank(&id)` | `tree.ranks().get(&id)`, or `rank` in `view` |
| `tree.missing(&id, &held)` | `tree.evaluate(&id, &held).map(\|e\| e.missing())` |
| `Runner::drain_events` and `Context::note` | logging in the host's `match` |
| `FlowModel` | `Library::validate`, which reports steps that never run |

## 0.3.0 — 2026-09-14

### Added

- `Requirement<K>`: a boolean rule held as data, over capability keys the
  host defines. It covers `Has`, `All`, `Any`, `Not`, `AtLeast`, `Flag`,
  and `Named`.
- `CapabilitySet<K>`: the capabilities one entity holds.
- `Evaluation<K>`: the result tree. It keeps the keys, reports
  `satisfied()`, and lists a conservative `missing()`.
- `Evaluation` implements `Display`, which draws the result as a ✓/✗
  tree.
- `conditions::Checks`: a registry that gives a host closure a name, so a
  stored requirement can call it through `Requirement::Named`.
- `effects::grant` and `effects::revoke` move one capability in or out of
  a `CapabilitySet`.
- `Unlocks<I, K>` and `Unlock<K>`: a graph of nodes that require
  capabilities and grant them. The edges are derived, never authored.
  It reports `is_available`, `available`, `providers`, `dependencies`,
  `optional_dependencies`, `rank`, and `ranks`, and `validate` finds
  cycles and unreachable content.
- `Requirement::required` and `Requirement::optional`: the hard and soft
  edge sets of a dependency graph.
- Optional `serde` feature for `Requirement`, `CapabilitySet`,
  `Evaluation`, `Unlock`, and `Unlocks`. Rules and whole trees are
  authorable in JSON, RON, or YAML.
- `examples/unlocks.rs` and `examples/techtree.rs`.

### Removed

- `conditions::All`, `Any`, `Not`, `Flag`, and `Always`, with the `all`,
  `any`, `not`, `flag`, and `flag_clear` shorthands. `Requirement`
  replaces them with one vocabulary that serializes and inspects.
  Replace `conditions::Flag::is_set("x")` with `Rule::flag("x")`, and
  `conditions::Always::default()` with `Rule::all([])`.

### Note

`Requirement::evaluate` takes a capability set. Use `satisfies_in` for
the step-context answer, because the inherent method shadows
`Condition::evaluate`.

## 0.2.0 — 2026-08-24

- Built-in `Goto` step and early `Return` control flow.
- Closure-backed conditions and effects, with constructor shorthands for
  every built-in.
- Typed context accessors and the conditional `when` step wrapper.
- Whole-library validation, library name lookup, and nested authoring
  warnings.
- `StepFacts::references` exposes every outgoing sequence reference to
  tooling.
- Faster runtime path that avoids building full step facts; fixed
  zero-capacity event buffering.

## 0.1.0 — 2026-08-23

Initial release.

- Sequences, steps, and a runner with call chains and external waits.
- Conditions and effects connecting host systems.
- `Completion` one-shot wait handles.
- Flow-model reachability analysis.
- `no_std` support with optional `std` panic isolation.
