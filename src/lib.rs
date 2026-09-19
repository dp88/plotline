//! Authored sequences, the rules that gate them, and unlock trees, as plain
//! data for any engine.
//!
//! A sequence says "this, then this, then this":
//!
//! ```text
//! hub:        Say "Have you found my ring?"
//!             When the player carries the ring: Goto returned
//!             Choose "Where did you lose it?" -> hint
//!                    "Not yet."               -> later
//!
//! returned:   Shake the ground
//!             Take the ring
//!             Say "You have my thanks."
//! ```
//!
//! Use it for dialog, quests, cutscenes, tutorials, and any other authored
//! flow. The same rules gate technology and ability trees.
//!
//! # The pieces
//!
//! - [`Step`] is one step of a sequence, as data. [`Library`] holds named
//!   sequences, and [`SequenceRef`] is a sequence name.
//! - [`Runner`] walks the steps. It hands each action to the host and waits
//!   for an [`Answer`].
//! - [`Requirement`] is a rule as data. [`Has`] answers each key it asks
//!   about, and [`Evaluation`] explains the answer.
//! - [`Unlocks`] is a graph of nodes that require keys and grant them.
//!   [`Unlocks::view`] gives a tree view everything it draws.
//!
//! # The host's loop
//!
//! `A` is the host's own action type: a line of dialog, a camera shake, a
//! grant. The crate never interprets an action. [`Runner::advance`] runs
//! control flow until it reaches a [`Step::Act`], and returns its action in
//! [`Status::Act`]. The host performs the action in its own code, then calls
//! [`Runner::resume`] with an answer:
//!
//! ```
//! # use std::collections::BTreeSet;
//! # use plotline::{Answer, Library, Runner, Status, Step};
//! # enum Action { Say(&'static str) }
//! # let mut library: Library<Action, ()> = Library::new();
//! # library.insert("hub", [Step::act(Action::Say("Hello."))]);
//! # let held = BTreeSet::new();
//! # let mut runner = Runner::default();
//! # runner.start("hub").unwrap();
//! let mut status = runner.advance(&library, &held);
//! while let Status::Act(action) = status {
//!     let answer = match action {
//!         Action::Say(line) => {
//!             println!("{line}");
//!             Answer::Done
//!         }
//!     };
//!     status = runner.resume(answer, &library, &held);
//! }
//! ```
//!
//! Every effect lives in that `match`, so a test can drive a script with a
//! fake world and check the list of actions.
//!
//! # Waiting
//!
//! The crate has no clock. An action waits for as long as the host takes to
//! answer it. A frame-based host keeps the [`Status`] or calls
//! [`Runner::advance`] once per frame, which returns [`Status::Waiting`] and
//! changes nothing until the host calls [`Runner::resume`].
//!
//! # Control flow
//!
//! [`Step::When`] runs one step only when a rule holds. [`Step::Branch`]
//! starts one of two sequences. [`Step::Call`] runs a sequence and then
//! continues, and falling off the end of a sequence returns to its caller.
//! [`Step::Return`] leaves a sequence early. [`Step::Goto`] clears the call
//! stack and starts a sequence, or ends the chain.
//!
//! An [`Answer`] can steer the chain in the same ways. A dialog choice is an
//! action whose answer is [`Answer::Goto`] to the branch the player picked.
//!
//! # Rules
//!
//! A [`Requirement`] is a tree of [`Requirement::Has`] leaves under `All`,
//! `Any`, `Not`, and `AtLeast`, over a key type `K` that the host chooses.
//! The crate never interprets a key. It asks a [`Has`] holder about each
//! one. A plain `BTreeSet<K>` is a holder. A host type can implement [`Has`]
//! itself, to store some keys and compute others, such as "gold is at least
//! 500". Every rule, gate, and tree query then sees one answer for a key.
//!
//! [`Requirement::satisfies`] answers with a bool and allocates nothing.
//! [`Requirement::evaluate`] returns an [`Evaluation`] tree that explains the
//! answer. It prints as a ✓/✗ tree and lists the conservative
//! [`Evaluation::missing`] keys. [`Requirement::warning`] reports rules that
//! are legal but almost certainly a mistake.
//!
//! # Trees
//!
//! [`Unlocks`] holds nodes. Each [`Unlock`] requires some keys and grants
//! others. Nobody authors the edges: one node grants a key, another requires
//! it, and that is the arrow. So a rule and its graph cannot drift apart.
//!
//! [`Unlocks::view`] returns what a tree view draws: each node's
//! [`NodeStatus`] (unlocked, available, or locked), its layout column, its
//! solid edges, and its dashed edges. A solid edge comes from the only node
//! that grants a required key. A dashed edge comes from one of several
//! alternatives. [`Unlocks::evaluate`] explains a locked node, and
//! [`Unlocks::take`] applies a node's grants. The host keeps its own record
//! of the nodes it took, because two nodes can grant the same key.
//!
//! # Validation
//!
//! [`Library::validate`] reports steps that name a missing sequence, rules
//! that report a warning, and the first step of each tail that never runs.
//! Cycles in a library are valid, because a hub that loops is a normal
//! dialog.
//!
//! [`Unlocks::validate`] reads required keys only. It reports a required key
//! that nothing grants, and a node whose required keys no order of takes
//! supplies. It does not check the keys inside `Any` or `AtLeast`.
//!
//! # Limits and errors
//!
//! Content can loop without an action, for example two sequences that jump
//! to each other. [`Limits`] caps the call depth and the steps that one
//! advance runs, and the runner returns [`Status::Aborted`] with the reason.
//! A missing sequence aborts the chain with its name.
//!
//! The runner calls host code only through [`Has::has`]. It catches no
//! panic. A panic in `has` unwinds through the call to the runner, and the
//! cursor stays on the step whose rule asked.
//!
//! # Files and saves
//!
//! The `serde` feature derives `Serialize` and `Deserialize` for the data
//! types. A [`Library`] loads from JSON, RON, or YAML as a map from each
//! sequence name to its steps. A step names its targets, so a file can refer
//! to a sequence before it defines it. Loading fails on a field the type does
//! not know and on a name that appears twice.
//!
//! A [`Runner`] saves in the middle of a chain. The save holds the stack of
//! positions, and not the limits. After a load, [`Runner::waiting_on`]
//! returns the action to show again. A saved runner resumes against the same
//! library it ran on.
//!
//! # Feature flags
//!
//! The crate is `no_std` and needs `alloc`. It has no required
//! dependencies. The one feature, `serde`, is off by default and works
//! without `std`.

#![no_std]

extern crate alloc;

mod rules;
mod runner;
mod sequence;
#[cfg(feature = "serde")]
mod unique_map;
mod unlocks;

pub use rules::{Evaluation, Has, Requirement};
pub use runner::{Abort, Answer, Busy, Limits, Runner, Status};
pub use sequence::{Library, Problem, SequenceRef, Step, Warning};
pub use unlocks::{NodeStatus, NodeView, Unlock, UnlockWarning, Unlocks};

/// The README is compiled as part of the test suite, so its examples cannot rot.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
