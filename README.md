![plotline banner](art/banner.webp)

# plotline

[![CI](https://github.com/dp88/plotline/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/plotline/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/plotline.svg)](https://crates.io/crates/plotline)
[![docs.rs](https://img.shields.io/docsrs/plotline)](https://docs.rs/plotline)
![MSRV](https://img.shields.io/badge/rust-1.85%2B-blue)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Capability requirements and unlock graphs as plain data, for any engine.
`plotline` answers what gates authored content: does an entity hold what a
thing demands, and if not, what is it short of?

## Quick start

```toml
[dependencies]
plotline = "0.3"
```

A requirement is a rule held as data, over capability keys you define. The
crate never interprets a key. It answers whether a holder has one, and
explains what failed. A plain `BTreeSet` is a holder, and so is any type
that implements `Has`.

```rust
use std::collections::BTreeSet;
use plotline::Requirement;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    FrozenHabitation,
    HighGravityHabitation,
    Shipyard,
    AlliedFleet,
}

let humans = BTreeSet::from([Cap::FrozenHabitation, Cap::Shipyard]);

let deneb_iv = Requirement::all([
    Requirement::has(Cap::FrozenHabitation),
    Requirement::has(Cap::HighGravityHabitation),
    Requirement::any([
        Requirement::has(Cap::Shipyard),
        Requirement::has(Cap::AlliedFleet),
    ]),
]);

assert!(!deneb_iv.satisfies(&humans));

let result = deneb_iv.evaluate(&humans);
assert_eq!(result.missing(), vec![Cap::HighGravityHabitation]);
```

Use it for technology prerequisites, equipment requirements, habitability,
crafting recipes, policies, dialogue choices, and skill unlocks.

## Trees

`Unlocks` collects nodes that require capabilities and grant them. Nobody
authors the edges. One node grants a key, another requires it, and that is
the arrow, so a rule and its graph can never drift apart.

```rust
use std::collections::BTreeSet;
use plotline::{NodeStatus, Requirement, Unlock, Unlocks};

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
`validate` reports cycles and content nothing can reach.

## Why

- **Rules you can read.** A `Requirement` clones, compares, prints, and
  serializes. Tools can walk it without running it.
- **Answers that explain themselves.** An `Evaluation` prints as a ✓/✗ tree
  and lists the conservative shortfall.
- **Graphs you do not maintain.** An unlock graph is derived from the rules
  themselves, so it cannot disagree with them.
- **No engine or runtime dependencies.** `no_std` with `alloc`.

## Requirements and features

- Rust 1.85 or later, edition 2024.
- `no_std` with `alloc`; no required dependencies.
- `serde` (off): derives `Serialize` and `Deserialize` for `Requirement`,
  `Evaluation`, `Unlock`, and `Unlocks`. It works without `std`.

## More examples and documentation

- [API documentation](https://docs.rs/plotline) — rustdoc is the manual.
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
