# Changelog

All notable changes to this project are documented in this file.

## Unreleased

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
