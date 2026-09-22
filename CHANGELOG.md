# Changelog

All notable changes to this project are documented in this file.

## Unreleased

## 0.3.0 — 2026-09-22

A rebuild around one idea: a step is data. The runner walks the steps and
hands each action to the host, and the host answers. Rules and unlock
trees join the sequences, and all of them read one vocabulary of keys. The
public surface drops from 64 items in 3 modules, 7 of them traits, to 19
items, 1 of them a trait.

### Added

- `Step<A, K>`: one step of a sequence, as data. `Act` hands the host's
  own action type `A` to the host. `When`, `Branch`, `Call`, `Goto`, and
  `Return` are control flow.
- `Runner::resume`, `Answer`, and `Status`: the runner hands out an action
  and waits until the host answers. An answer can also steer the chain,
  which is how a dialog choice jumps to the branch the player picked.
- `Requirement<K>`: a rule held as data. It covers `Has`, `All`, `Any`,
  `Not`, and `AtLeast` over a key type that the host defines.
- `Has<K>`: one trait answers every rule leaf. A `BTreeSet` is a holder,
  and a host type can store some keys and compute others.
- `Evaluation<K>`: the result tree. It reports `satisfied()`, lists a
  conservative `missing()`, and prints itself as a ✓/✗ tree.
- `Unlocks<I, K>` and `Unlock<K>`: a graph of nodes that require keys and
  grant them. Nobody authors the edges; the rules imply them. `view`
  returns each node's `NodeStatus`, rank, and edges in one call. `validate`
  reports required keys that nothing grants, and nodes that those keys or
  a loop block.
- The optional `serde` feature. Libraries, rules, trees, and runners load
  and save, and a runner saves in the middle of a chain. Loading fails on
  an unknown field and on a name that appears twice.
- `examples/unlocks.rs` and `examples/techtree.rs`.

### Changed

- `Library` maps sequence names to lists of steps. `SequenceRef` is a
  name, so a file can refer to a sequence before it defines it.
- `Runner::start` takes a sequence name. `Runner::advance` returns a
  `Status` in place of `Poll<Outcome>`. The runner holds only a stack of
  positions, so it clones, compares, and saves.
- The `Step` trait is now the `Step` enum.
- `Limits`, `Abort`, and `Busy` replace `RunnerConfig`, `AbortReason`, and
  `StartError`. One step limit, `steps_per_advance`, replaces the hop and
  resume limits.
- `Library::validate` takes a closure that lists the sequences an action
  names. It returns `Warning` values in place of `ValidationWarning`, and
  it reports missing sequences, rule warnings, and the first step of each
  tail that never runs.
- `examples/dialog.rs` drives a conversation with one `match`.

### Removed

- The closure API: `Sequence`, `Iter`, `RunnerEvent`, `Outcome`,
  `SkipReason`, `ChainGuard`, the `StepRun`, `IntoProgress`, `Condition`,
  `Effect`, `SequenceSource`, and `SequenceFacts` traits, `Progress`,
  `Flow`, `StepFacts`, `Context`, `TypeMap`, `ChainFlags`, `Completion`,
  `QueryCtx`, `EffectCtx`, `FlowModel`, `RailNode`, `RailShape`, and the
  `steps`, `conditions`, and `effects` modules.
- The `std` feature and panic isolation. The runner calls host code only
  through `Has::has`, and a panic there unwinds to the host.

### Migration

| 0.2 | 0.3 |
|---|---|
| `steps::run("Greet", \|ctx\| ...)` | an `Action` variant, `Step::act(Action::Greet)`, and a `match` arm in the host |
| A step that returns a `Completion` | an action that the host answers later |
| `Progress::Call`, `Goto`, or `Return` from a closure | `Answer::call`, `Answer::goto`, or `Answer::Return` |
| `library.insert(sequence)` and the handle it returns | `library.insert("name", steps)`, and steps that name their targets |
| `runner.start(handle, instigator)` | `runner.start("name")`; the host keeps its own instigator |
| `Poll<Outcome>` from `advance` | `Status` from `advance` and `resume` |
| Services in a `TypeMap` | the host's own state, read through its `Has` holder |
| `steps::Branch { condition, if_true, if_false }` | `Step::Branch { rule, if_true, if_false }` |
| `steps::when(condition, step)` | `Step::when(rule, step)` |
| `steps::goto`, `steps::stop`, `steps::Call`, `steps::Return` | `Step::goto`, `Step::stop`, `Step::call`, `Step::Return` |
| `conditions::all`, `any`, and `not` | `Requirement::all`, `any`, and `not` |
| `conditions::Always` | `Requirement::all([])` |
| `conditions::flag`, `flag_clear`, and the `SetFlag` step or effect | a key that the host's `Has` stores, and an action that stores it |
| `conditions::check` | a key that the host's `Has` computes |
| `effects::run` and `steps::ApplyEffects` | actions that the host performs |
| `steps::Note`, `Context::note`, and `Runner::drain_events` | logging in the host's `match` |
| `FlowModel` | `Library::validate`, which reports the first step of each tail that never runs |

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
