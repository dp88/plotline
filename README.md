![plotline banner](art/banner.webp)

# plotline

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/plotline.svg)](https://crates.io/crates/plotline)
[![docs.rs](https://img.shields.io/docsrs/plotline)](https://docs.rs/plotline)
![MSRV](https://img.shields.io/badge/rust-1.85%2B-blue)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Branching sequences of events as plain data — dialog, quests, cutscenes,
tutorials — for any engine. The host defines the steps; `plotline` runs
order, branches, subroutines, jumps, and waits.

It also answers what gates those flows: does an entity hold what a thing
demands, and if not, what is it short of?

## Quick start

```toml
[dependencies]
plotline = "0.3"
```

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

## Requirements

A requirement is a rule held as data, over capability keys you define. The
crate never interprets a key. It answers whether a set holds one, and
explains what failed.

```rust
use plotline::{CapabilitySet, Requirement};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    FrozenHabitation,
    HighGravityHabitation,
    Shipyard,
}

let humans = CapabilitySet::from([Cap::FrozenHabitation, Cap::Shipyard]);

let deneb_iv = Requirement::all([
    Requirement::has(Cap::FrozenHabitation),
    Requirement::has(Cap::HighGravityHabitation),
    Requirement::any([
        Requirement::has(Cap::Shipyard),
        Requirement::named("borrowed-fleet"),
    ]),
]);

assert!(!deneb_iv.satisfies(&humans));

let result = deneb_iv.evaluate(&humans);
assert_eq!(result.missing(), vec![Cap::HighGravityHabitation]);
```

The same value is a `Condition`, so a `Branch` or `when` step can hold one.
That path also reads chain flags and a `Checks` registry, which is how a
rule loaded from a file calls a host closure by name.

Use it for technology prerequisites, equipment requirements, habitability,
crafting recipes, policies, dialogue choices, and skill unlocks.

## Trees

`Unlocks` collects nodes that require capabilities and grant them. Nobody
authors the edges. One node grants a key, another requires it, and that is
the arrow, so a rule and its graph can never drift apart.

```rust
use plotline::{CapabilitySet, Requirement, Unlock, Unlocks};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    Fusion,
    Warp,
}

let mut tree = Unlocks::new();
tree.insert("fusion-power", Unlock::free().granting([Cap::Fusion]));
tree.insert(
    "warp-drive",
    Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
);

assert_eq!(tree.dependencies(&"warp-drive"), vec![&"fusion-power"]);
assert_eq!(tree.rank(&"warp-drive"), Some(1));

let mut held = CapabilitySet::new();
tree.take(&"fusion-power", &mut held);
assert!(tree.is_available(&"warp-drive", &held));
```

`rank` is the column a tree layout draws in. `validate` reports cycles and
content nothing can reach.

## Why

- **Plain data.** A sequence is an ordered list of steps the host defines —
  closures for the simple cases, structs where a step carries state.
- **Rules you can read.** A `Requirement` clones, compares, prints, and
  serializes. Tools can walk it without running it.
- **Graphs you do not maintain.** An unlock graph is derived from the rules
  themselves, so it cannot disagree with them.
- **Subroutines and jumps.** `Call` enters a subroutine and falling off its
  end returns to the caller; `Return` exits early; `Goto` clears the whole
  call chain before starting its target.
- **No clock.** A waiting step returns a `Completion` handle. The host
  signals it and calls `advance()`. The crate does not define timed waits.
- **No engine or runtime dependencies.** `no_std` with `alloc`; the default
  `std` feature adds panic isolation only.
- **Tooling-ready.** Whole-library validation, per-step reference facts, and
  reachability analysis feed editors and linters.

## Requirements and features

- Rust 1.85 or later, edition 2024.
- `no_std` with `alloc`; no required dependencies.
- `std` (default): catches panics in steps. It requires `panic = "unwind"`.
- `serde` (off): derives `Serialize` and `Deserialize` for `Requirement`,
  `CapabilitySet`, and `Evaluation`. It works without `std`.
- `--no-default-features` builds without `std` and does not catch panics.

## More examples and documentation

- [API documentation](https://docs.rs/plotline) — rustdoc is the manual:
  steps, built-ins, validation, analysis, and diagnostics.
- [`examples/dialog.rs`](examples/dialog.rs) — a branching conversation.
  Run it with `cargo run --example dialog`.
- [`examples/unlocks.rs`](examples/unlocks.rs) — requirements gating what an
  empire can do. Run it with `cargo run --example unlocks`.
- [`examples/techtree.rs`](examples/techtree.rs) — a technology tree drawn
  from derived edges. Run it with `cargo run --example techtree`.
- [CHANGELOG](CHANGELOG.md)
- [Issue tracker](https://github.com/dp88/plotline/issues)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

*Banner artwork: [“Subway” (1934) by Lily Furedi](https://commons.wikimedia.org/wiki/File%3ASubway%2C_Furedi%2C_1934.jpg),
via Wikimedia Commons. The digital image is credited to the Smithsonian
American Art Museum; the file is marked public domain.*
