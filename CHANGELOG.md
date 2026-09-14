# Changelog

All notable changes to this project are documented in this file.

## Unreleased

## 0.3.0 — 2026-09-13

### Added

- `Requirement<K>`: a boolean rule held as data, over capability keys the
  host defines. It covers `Has`, `All`, `Any`, `Not`, `AtLeast`, `Flag`,
  and `Named`.
- `CapabilitySet<K>`: the capabilities one entity holds.
- `Evaluation<K>`: the result tree. It keeps the keys, reports
  `satisfied()`, and lists a conservative `missing()`.
- `Condition::explain` returns an `Explanation`, the display tree for a
  boxed condition whose key type the caller does not know. The default
  reports one leaf, so existing conditions need no change.
- `conditions::Checks`: a registry that gives a host closure a name, so a
  stored requirement can call it through `Requirement::Named`.
- `effects::grant` and `effects::revoke` move one capability in or out of
  a `CapabilitySet`.
- Optional `serde` feature for `Requirement`, `CapabilitySet`, and
  `Evaluation`. Rules are authorable in JSON, RON, or YAML.
- `examples/unlocks.rs`.

### Removed

- `conditions::All`, `Any`, `Not`, and `Flag`, with the `all`, `any`,
  `not`, `flag`, and `flag_clear` shorthands. `Requirement` replaces them
  with one vocabulary that serializes and inspects. Replace
  `conditions::Flag::is_set("x")` with `Rule::flag("x")`.

### Note

`Requirement::evaluate` takes a capability set. Use `satisfies_in` and
`evaluate_in` for the step-context answer, because the inherent method
shadows `Condition::evaluate`.

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
