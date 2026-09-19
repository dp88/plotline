![plotline banner](art/banner.webp)

# plotline

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/plotline.svg)](https://crates.io/crates/plotline)
[![docs.rs](https://img.shields.io/docsrs/plotline)](https://docs.rs/plotline)
![MSRV](https://img.shields.io/badge/rust-1.85%2B-blue)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Authored sequences, the rules that gate them, and unlock trees, as plain
data for any engine. Use it for dialog, quests, cutscenes, tutorials, and
technology or ability trees.

A step is a value. The runner walks the steps and hands each action to your
code. Your code performs the action and answers, and the runner moves on.

## Quick start

```toml
[dependencies]
plotline = "0.4"
```

```rust
use std::collections::BTreeSet;
use plotline::{Answer, Library, Requirement, Runner, Status, Step};

// Your vocabulary. The crate never interprets it.
enum Action {
    Say(&'static str),
    Choose(Vec<&'static str>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    HasRing,
}

let mut library = Library::new();
library.insert("hub", [
    Step::act(Action::Say("Have you found my ring?")),
    Step::when(Requirement::has(Key::HasRing), Step::goto("thanks")),
    Step::act(Action::Choose(vec!["later", "hint"])),
]);
library.insert("thanks", [Step::act(Action::Say("You have my thanks."))]);
library.insert("later", [Step::act(Action::Say("Come back when you have."))]);
library.insert("hint", [Step::act(Action::Say("Near the old well."))]);

let held = BTreeSet::new(); // The player has no ring yet.
let mut runner = Runner::default();
runner.start("hub").unwrap();

let mut status = runner.advance(&library, &held);
while let Status::Act(action) = status {
    let answer = match action {
        Action::Say(line) => {
            println!("{line}");
            Answer::Done
        }
        // The player picks the second reply.
        Action::Choose(replies) => Answer::goto(replies[1]),
    };
    status = runner.resume(answer, &library, &held);
}
assert!(matches!(status, Status::Finished));
```

A wait is the gap between `advance` and `resume`, so the crate needs no
clock, callback, or async runtime.

## Trees

`Unlocks` holds nodes that require keys and grant them. Nobody authors the
edges. One node grants a key, another requires it, and that is the arrow,
so a rule and its graph cannot drift apart.

```rust
use std::collections::BTreeSet;
use plotline::{NodeStatus, Requirement, Unlock, Unlocks};

let mut tree = Unlocks::new();
tree.insert("fusion-power", Unlock::free().granting(["fusion"]));
tree.insert(
    "warp-drive",
    Unlock::new(Requirement::has("fusion")).granting(["warp"]),
);

let mut held = BTreeSet::new();
let mut taken = BTreeSet::new();
tree.take(&"fusion-power", &mut held);
taken.insert("fusion-power");

for node in tree.view(&held, &taken) {
    let mark = match node.status {
        NodeStatus::Unlocked => '●',
        NodeStatus::Available => '○',
        NodeStatus::Locked => '·',
    };
    println!("{:?} {mark} {} after {:?}", node.rank, node.id, node.dependencies);
}
```

`view` gives each node's status, layout column, and edges in one call.
`validate` checks the required keys of each node. It reports a key that
nothing grants, and a node that a loop or a missing key blocks.

## Why

- **Plain data.** A script is a list of `Step` values. It prints, compares,
  validates, and loads from JSON, RON, or YAML.
- **Your loop, your code.** The runner reads keys through your `Has` holder
  and hands every action back to you. Every effect lives in one `match` in
  the host, so a test can check the actions a script produces.
- **One answer per key.** Stored and computed keys go through one `Has`
  trait, so a dialog gate, a tooltip, and a tech tree always agree.
- **Rules that explain themselves.** An `Evaluation` prints as a ✓/✗ tree
  and lists what is missing.
- **Small.** 19 public items, `no_std`, no required dependencies. Runaway
  content aborts instead of hanging, and a runner saves in the middle of a
  chain.

## Requirements and features

- Rust 1.85 or later, edition 2024.
- `no_std` with `alloc`; no required dependencies.
- `serde` (off): derives `Serialize` and `Deserialize` for the data types,
  including `Library` and `Runner`. Loading fails on an unknown field and on
  a name that appears twice. It works without `std`.

## More examples and documentation

- [API documentation](https://docs.rs/plotline) — rustdoc is the manual.
- [`examples/dialog.rs`](examples/dialog.rs) — a conversation with a choice,
  an event, and a quest gate. Run it with `cargo run --example dialog`.
- [`examples/unlocks.rs`](examples/unlocks.rs) — requirements that explain
  themselves and gate a sequence. Run it with `cargo run --example unlocks`.
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
