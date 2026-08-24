![plotline banner](art/banner.webp)

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Branching sequences of events as plain data. Use it for dialog, quests, and more.

## What `plotline` does

A sequence is an ordered list of steps:

```text
say "Hello, traveler."
say "Have you seen my ring?"
[choice] "Yes"  ──▶ ring_found
         "No"   ──▶ ring_lost

ring_found:  remove item "gold ring"
             advance quest "The Lost Ring" to stage 2
             say "You have my thanks."
```

`plotline` handles order, branches, calls, returns, jumps, and waits. The host defines the
steps.

`Call` enters a subroutine and falling off its end returns to the caller. `Return` exits the
current subroutine early. `Goto` clears the whole call chain before starting its target (or ends
the chain when it has no target).

## What it does not do

**No clock.** A step that needs to wait returns `Progress::Wait` with a [`Completion`] handle.
The host signals the handle and calls `advance()`. A multi-phase step can instead return
`Progress::Resume` and manage its own per-run state. The crate does not define timed waits.

**No engine or runtime dependencies.** The crate uses `alloc` and supports `no_std`.

## Example

```rust
use core::task::Poll;
use plotline::{Completion, Library, Outcome, Runner, Sequence, TypeMap, steps};

let ready = Completion::new();
let waiting_on = ready.clone();

let mut library = Library::new();
let farewell = library.insert(
    Sequence::new("farewell")
        .with_step(steps::run("Say goodbye", |_ctx| println!("Safe roads."))),
);
let greeting = library.insert(
    Sequence::new("greeting")
        .with_step(steps::run("Say hello", |_ctx| println!("Hello, traveler.")))
        .with_step(steps::run("Wait for the world", move |_ctx| waiting_on.clone()))
        .with_step(steps::Branch {
            condition: None,
            if_true: Some(farewell),
            if_false: None,
        }),
);

let mut runner = Runner::default();
let mut services = TypeMap::new();
runner.start(greeting, None).unwrap();

assert_eq!(runner.advance(&mut library, &mut services), Poll::Pending);

ready.signal();
assert_eq!(
    runner.advance(&mut library, &mut services),
    Poll::Ready(Outcome::Finished),
);
```

Run the example:

```console
$ cargo run --example dialog
```

## API reference

The public API has data types, execution traits, and built-in steps.

### 📚 Data types

#### 🧮 Sequences

| Type | Description | Main API |
| --- | --- | --- |
| `Sequence` | Ordered list of steps. | `new`, `with_step`, `push`, `insert`, `remove`, `get`, `iter` |
| `Iter` | Iterator over sequence steps. | `Iterator`, `ExactSizeIterator` |
| `Library` | Owns sequences and creates their handles. | `insert`, `get`, `get_mut`, `find`, `ref_by_name`, `validate`, `iter` |
| `SequenceRef` | Opaque sequence handle. | `from_raw`, `to_raw` |

#### 🐾 Step execution

| Type | Description | Main API |
| --- | --- | --- |
| `Flow` | Declared continuation after a step. | `Continue`, `End` |
| `Progress` | Result of starting or resuming one step. | `Done`, `Wait`, `Call`, `Return`, `Goto`, `Resume` |
| `Completion` | Thread-safe, one-shot wait handle. | `new`, `done`, `signal`, `is_complete` |
| `StepFacts` | Snapshot used by analysis, editors, and tools. | `of`, `references` |

#### 🧩 Context & state

| Type | Description | Main API |
| --- | --- | --- |
| `Context` | Services, chain state, and location for one step. | `services`, `services_mut`, `service`, `service_mut`, `flags`, `flags_mut`, `flag`, `set_flag`, `eval`, `enact`, `note`, `location`, `instigator`, `instigator_as` |
| `TypeMap` | One host value per concrete `'static` type. | `insert`, `get`, `get_mut`, `remove` |
| `ChainFlags` | Boolean flags shared by one chain. | `flag`, `set_flag` |
| `QueryCtx` | Read-only context for a condition. | `target`, `target_as`, `chain`, `caps`, `service` |
| `EffectCtx` | Mutable context for an effect. | `target`, `target_as`, `chain`, `caps`, `service`, `service_mut` |

#### 👟 Runner

| Type | Description | Main API |
| --- | --- | --- |
| `Runner` | Executes one sequence chain at a time. | `start`, `advance`, `stop`, `is_running`, `current`, `drain_events` |
| `RunnerConfig` | Limits for call depth, hops, resumes, and events. | Chainable limit setters |
| `RunnerEvent` | Diagnostic event emitted by the runner. | `StepStarted`, `StepFailed`, `StepSkipped`, `SequenceMissing`, `Note` |
| `ChainGuard` | Type alias for a host value held while a chain runs. | Dropped when the chain ends |
| `Outcome` | Result returned by `Runner::advance`. | `Idle`, `Finished`, `Aborted` |
| `AbortReason` | Reason for a guarded abort. | `HopLimit`, `ResumeLimit`, `CallDepth` |
| `SkipReason` | Reason a step was skipped. | `Disabled`, `Missing`, `Vanished` |
| `StartError` | Reason `Runner::start` failed. | `AlreadyRunning` |

#### 🔬 Analysis

| Type | Description | Main API |
| --- | --- | --- |
| `FlowModel` | Reachability analysis for one sequence. | `analyse`, terminal and warning queries |
| `RailNode` | Display data for one analysed step. | `shape`, `solid`, `terminal`, `severed`, `soften_below` |
| `RailShape` | Shape for a flow-analysis node. | `Circle`, `Diamond` |
| `ValidationWarning` | One warning from whole-library validation. | `sequence`, `index`, `message` |

### 🧬 Traits

| Trait | Purpose | Key API |
| --- | --- | --- |
| `Step` | Defines one host operation. | `summary`, `start`, `references` |
| `StepRun` | Stores per-run state for a multi-phase step. | `resume` |
| `IntoProgress` | Converts closure results to `Progress`. | `into_progress` |
| `Condition` | Reads state and returns a Boolean answer. | `summary`, `evaluate` |
| `Effect` | Applies one effect in one call. | `summary`, `apply` |
| `SequenceFacts` | Supplies sequence facts to analysis, editors, and tools. | `step_count`, `step_facts`, `step_enabled`, `name` |
| `SequenceSource` | Supplies sequence data and starts steps. | `start_step` |

### Built-ins

Use the modules by name:

- `conditions`: `Always`, `Not`, `All`, `Any`, `Flag`, `Check`, `check`, `not`, `all`, `any`, `flag`, and `flag_clear`.
- `effects`: `SetFlag`, `Run`, `run`, `set_flag`, and `clear_flag`.
- `steps`: `Note`, `SetFlag`, `Branch`, `Goto`, `Call`, `Return`, `Stop`, `ApplyEffects`, `When`, `Run`, `run`, `branch`, `goto`, `call`, `stop`, and `when`.

`steps::run` wraps a closure. Its body can return `()`, a `Completion`, or `Progress`; other
return types are also supported when they implement [`IntoProgress`].

`conditions::check` and `effects::run` provide the same closure-first style for conditions and
effects. `steps::when` conditionally runs any step. Built-in constructors such as
`conditions::flag`, `effects::set_flag`, `steps::call`, and `steps::goto` are shorthand over the
public structs and do not remove the struct-literal API.

`Library::validate()` reports empty or duplicate names, step and nested-object warnings, and
references to missing sequences. It intentionally permits cycles, which are valid for authored
graphs. `StepFacts::references` exposes every outgoing sequence reference, including both sides of
a branch.

```rust
# use plotline::{Sequence, steps};
Sequence::new("greeting")
    .with_step(steps::run("Greet the elder", |_ctx| println!("Hello.")))
    .with_step(steps::run("Remember it", |ctx| ctx.set_flag("greeted", true)));
```

`Condition` and `Effect` connect the host systems. They can be used outside the runner.

Run `cargo doc --open` for the API.

## Diagnostics

The runner reports events. The host decides how to log them:

```rust
# use plotline::Runner;
# let mut runner = Runner::default();
for event in runner.drain_events() {
    println!("{event:?}");
}
```

`ctx.note(..)` adds location-tagged notes.

## Requirements

- Rust 1.85 or later (edition 2024).
- **`panic = "unwind"`** is required for panic isolation with the default `std` feature.
- `--no-default-features` builds without `std` and does not catch panics.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE)
