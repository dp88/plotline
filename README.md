# plotline

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Branching sequences of events as plain data. Use it for dialog, quests, and cutscenes.

## What it does

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

plotline owns order, branches, calls, and waits. The host defines the steps.

## What it does not do

**No clock.** A step that cannot finish returns a [`Completion`] handle. The host signals
the handle and calls `advance()`. The crate does not define timed waits.

**No engine or runtime dependencies.** The crate uses `alloc` and supports `no_std`.

## API reference

The public API has data types, execution traits, and built-in steps.

### Data types

| Type | Description | Main operations |
| --- | --- | --- |
| `Completion` | Thread-safe, one-shot wait handle. | `new`, `done`, `signal`, `is_complete` |
| `TypeMap` | One host value per Rust type. | `insert`, `get`, `get_mut`, `remove` |
| `ChainFlags` | Boolean flags shared by one chain. | `flag`, `set_flag` |
| `Context` | Services and chain state for one step. | `services`, `flags`, `eval`, `enact`, `note` |
| `QueryCtx` | Read-only context for a condition. | `target`, `chain`, `caps` |
| `EffectCtx` | Mutable context for an effect. | `target`, `chain`, `caps` |
| `SequenceRef` | Opaque sequence handle. | `from_raw`, `to_raw` |
| `Sequence` | Ordered list of shared steps. | `new`, `with_step`, `push`, `iter` |
| `Iter` | Iterator over sequence steps. | `Iterator`, `ExactSizeIterator` |
| `Library` | Owns sequences and their handles. | `insert`, `get`, `get_mut`, `iter` |
| `RunnerConfig` | Limits for recursion, hops, resumes, and events. | Chainable limit setters |
| `Runner` | Executes one sequence chain at a time. | `start`, `advance`, `stop`, `current`, `drain_events` |
| `RunnerEvent` | Diagnostic event emitted by the runner. | `StepStarted`, `StepFailed`, `StepSkipped`, `SequenceMissing`, `Note` |
| `ChainGuard` | Host value held while a chain runs. | Dropped when the chain ends |
| `Outcome` | Result returned by `Runner::advance`. | `Idle`, `Finished`, `Aborted` |
| `AbortReason` | Reason for a guarded abort. | `HopLimit`, `ResumeLimit`, `CallDepth` |
| `SkipReason` | Reason a step was skipped. | `Disabled`, `Missing`, `Vanished` |
| `StartError` | Reason `Runner::start` failed. | `AlreadyRunning` |
| `Flow` | Declared continuation after a step. | `Continue`, `End` |
| `Progress` | Result of one step call. | `Done`, `Wait`, `Call`, `Goto`, `Resume` |
| `StepFacts` | Snapshot used by analysis and tools. | `of` |
| `RailShape` | Shape for a flow-analysis node. | `Circle`, `Diamond` |
| `RailNode` | Display data for one analysed step. | `shape`, `solid`, `terminal`, `severed`, `soften_below` |
| `FlowModel` | Reachability analysis for one sequence. | `analyse`, terminal and warning queries |

### Traits

| Trait | Purpose | Required operation |
| --- | --- | --- |
| `Step` | Defines one host operation. | `summary`, `start` |
| `StepRun` | Stores state for a multi-phase step. | `resume` |
| `IntoProgress` | Converts closure results to `Progress`. | `into_progress` |
| `Condition` | Reads state and returns a Boolean answer. | `summary`, `evaluate` |
| `Effect` | Performs one immediate state change. | `summary`, `apply` |
| `SequenceFacts` | Supplies sequence data to analysis. | `step_count`, `step_facts`, `name` |
| `SequenceSource` | Supplies sequence data and starts steps. | `start_step` |

### Built-ins

Use the modules by name:

- `conditions`: `Always`, `Not`, `All`, `Any`, and `Flag`.
- `effects`: `SetFlag`.
- `steps`: `Note`, `SetFlag`, `Branch`, `Call`, `Stop`, `ApplyEffects`, `Run`, and `run`.

`steps::run` wraps a closure. Its body returns `()`, a `Completion`, or `Progress`.

```rust
# use plotline::{Sequence, steps};
Sequence::new("greeting")
    .with_step(steps::run("Greet the elder", |_ctx| println!("Hello.")))
    .with_step(steps::run("Remember it", |ctx| ctx.set_flag("greeted", true)));
```

`Condition` and `Effect` connect the host systems. They can be used outside the runner.

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

MIT or Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

[`Completion`]: https://docs.rs/plotline/latest/plotline/struct.Completion.html
