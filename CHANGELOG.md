# Changelog

All notable changes to this project are documented in this file.

## Unreleased

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
