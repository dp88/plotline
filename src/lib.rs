//! Capability requirements and unlock graphs for authored game logic.
//!
//! The crate answers one question: does an entity hold what a thing demands,
//! and if not, what is it short of?
//!
//! ```text
//! Deneb IV requires
//!     ✓ FrozenHabitation
//!     ✗ HighGravityHabitation
//! ```
//!
//! # The pieces
//!
//! [`Requirement`] holds a boolean rule as data. [`Has`] answers whether an
//! entity holds a key; a plain `BTreeSet` of keys is the simplest holder.
//! [`Unlocks`] joins many rules into a graph. Together they cover
//! prerequisites, technology trees, permissions, and habitability.
//!
//! # Requirements
//!
//! A [`Requirement`] is a tree over capability keys the host chooses. The
//! crate never interprets a key. [`Requirement::satisfies`] answers with a
//! bool and allocates nothing; [`Requirement::evaluate`] returns an
//! [`Evaluation`] tree that explains the answer, including the conservative
//! [`Evaluation::missing`] list. Both are pure functions of the requirement
//! and the holder, so they suit a user interface, a planner, or a test.
//! [`Requirement::warning`] reports rules that are legal but almost certainly
//! a mistake.
//!
//! A host type can implement [`Has`] itself. It then answers some keys from
//! stored state and computes others, such as "gold is at least 500". Every
//! rule, graph query, and evaluation tree sees the same answer for a key.
//!
//! # Unlock graphs
//!
//! [`Unlocks`] holds nodes that require capabilities and grant them. Nobody
//! authors its edges. One node grants a key, another requires it, and that is
//! the arrow, so a requirement and its graph can never drift apart.
//!
//! [`Requirement::required`] gives the keys a rule demands outright and
//! [`Requirement::optional`] gives the keys inside a choice. Those are the
//! solid and dashed edges of a technology tree.
//!
//! [`Unlocks::view`] returns what a tree view draws: each node's
//! [`NodeStatus`] (unlocked, available, or locked), its layout column, and
//! both kinds of edge. [`Unlocks::evaluate`] explains a locked node, and
//! [`Unlocks::take`] applies a node's grants. [`Unlocks::validate`] reports
//! cycles and content nothing can reach.
//!
//! # Feature flags
//!
//! The crate is `no_std` and needs `alloc`. The `serde` feature derives
//! `Serialize` and `Deserialize` for [`Requirement`], [`Evaluation`],
//! [`Unlock`], and [`Unlocks`], so rules are authorable in JSON, RON, or
//! YAML.

#![no_std]

extern crate alloc;

mod rules;
mod runner;
mod sequence;
mod unlocks;

pub use rules::{Evaluation, Has, Requirement};
pub use runner::{Abort, Answer, Busy, Limits, Runner, Status};
pub use sequence::{Library, Problem, SequenceRef, Step, Warning};
pub use unlocks::{NodeStatus, NodeView, Unlock, UnlockWarning, Unlocks};

/// The README is compiled as part of the test suite, so its examples cannot rot.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
