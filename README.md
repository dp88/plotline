# plotline

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Branching sequences of events, as plain data. For dialog trees, quests, cutscenes, and
anything else that is "this, then this, then — depending on the answer — that".

I wrote it to stop hand-rolling a dialog tree every time I hack on a game.

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

plotline owns the *shape* — the order, the branches, the subroutine calls, the waiting.
Your game owns the *verbs*. "Say a line", "remove an item", "advance a quest" are steps
you write, and plotline never learns what they mean.

## What it does not do

**It has no clock.** No frames, no delta time, no `async`. A step that cannot finish now
returns a [`Completion`] handle, and something outside the crate — a dialog panel, an
animation, a timer your engine owns — signals it. You call `advance()` whenever that may
have happened. Between calls the runner is inert data.

That is why there is no "wait 2 seconds" step built in: seconds belong to whoever owns the
clock. The same crate runs under a game loop, a CLI, and a unit test without knowing which.

**It has no engine, and no dependencies.** Zero. It is `no_std` too — `#![no_std]` plus
`alloc`, verified by cross-compiling to bare-metal ARM in CI.

## The API in one screen

| Type | What it is |
| --- | --- |
| `Sequence` | An ordered list of steps. A value: build it, iterate it, analyse it. |
| `Library` | Owns sequences and mints the `SequenceRef` handles they use to link to each other. |
| `Runner` | Walks a sequence, its branches, and its subroutines. Holds all run state. |
| `Step` | Your verb. `summary()` and `start()` — that is the whole contract. |
| `Condition` / `Effect` | Ask the world a yes/no question; act on the world instantly. |
| `Completion` | The one wait primitive. One-shot, idempotent, safe to signal from any thread. |
| `FlowModel` | Reachability analysis: which steps can run, where a sequence certainly ends. |

Built-ins live in `steps`, `conditions`, and `effects` — `steps::Branch`, `steps::Call`,
`conditions::Flag`, `effects::SetFlag`, and a handful more.

**A step is usually a closure.** `steps::run` takes a name and a body, and the body
answers with whatever fits: nothing at all to finish now, a `Completion` to wait on, or a
`Progress` when it needs control flow. Writing a `struct` and an `impl` block is for steps
that carry authored data.

```rust
# use plotline::{Sequence, steps};
Sequence::new("greeting")
    .with_step(steps::run("Greet the elder", |_ctx| println!("Hello.")))
    .with_step(steps::run("Remember it", |ctx| ctx.set_flag("greeted", true)));
```

`Condition` and `Effect` are the seam between systems, not the runner. A quest system
contributes "quest is at stage 3"; an inventory contributes "has item". A branch step uses
them, and so does a dialog choice, a trigger volume, and an item's use handler — none of
which know the runner exists.

## Example

```rust
use core::task::Poll;
use plotline::{Completion, Library, Outcome, Runner, Sequence, TypeMap, steps};

// Something outside signals this: a dialog panel, an animation, a timer your
// engine owns. From plotline's point of view they all look the same.
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
        // Returning a Completion means "wait on this". No `Progress` in sight.
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

// The chain holds at the wait...
assert_eq!(runner.advance(&mut library, &mut services), Poll::Pending);

// ...until the world signals — from wherever, whenever "the world" is.
ready.signal();
assert_eq!(
    runner.advance(&mut library, &mut services),
    Poll::Ready(Outcome::Finished),
);
```

There is a runnable version of the conversation at the top of this file in
[`examples/dialog.rs`](examples/dialog.rs) — cycles, services, and all:

```console
$ cargo run --example dialog
```

Run `cargo doc --open` for the rest. Every public item is documented.

## Diagnostics

The crate owns no logger, which is why it has no dependencies. The runner reports what it
did as events, and you forward them wherever you like:

```rust
# use plotline::Runner;
# let mut runner = Runner::default();
for event in runner.drain_events() {
    println!("{event:?}"); // or log::info!, or tracing::event!
}
```

Steps say things with `ctx.note(..)`, tagged with where they said it. The catch is real:
a host that never drains sees nothing.

## Requirements

- Rust 1.85 or later (edition 2024).
- **`panic = "unwind"`, if you want panic isolation.** With the default `std` feature the
  runner catches a panicking step, reports it, and carries on. That needs an unwinder, so
  `panic = "abort"` turns the isolation into process death.
- `--no-default-features` gives you the `no_std` build. It makes the same bargain as
  `panic = "abort"`: no isolation, because bare metal has no unwinder to offer.

## Status and honesty

Version 0.1. The API will move.

Two things you should know before you depend on this:

- **It is AI-assisted code.** I directed the design and reviewed the result, but I did not
  type most of it.
- **I am a professional developer and a Rust novice.** The design I stand behind. The
  idiom may well be off, and there are almost certainly Rust things I did wrong.

Both are reasons to read the source before you trust it — and reasons an issue or a PR is
genuinely welcome. It is a small crate: about 4,000 lines including tests.

## License

MIT or Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

[`Completion`]: https://docs.rs/plotline/latest/plotline/struct.Completion.html
