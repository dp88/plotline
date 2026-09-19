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
//! [`Requirement`] holds a boolean rule as data. [`CapabilitySet`] holds what
//! an entity has. [`Unlocks`] joins many of them into a graph. Together they
//! cover prerequisites, technology trees, permissions, and habitability.
//!
//! # Requirements
//!
//! A [`Requirement`] is a tree over capability keys the host chooses. The
//! crate never interprets a key. [`Requirement::satisfies`] answers with a
//! bool and allocates nothing; [`Requirement::evaluate`] returns an
//! [`Evaluation`] tree that explains the answer, including the conservative
//! [`Evaluation::missing`] list. Both are pure functions of the requirement
//! and the [`CapabilitySet`], so they suit a user interface, a planner, or a
//! test. [`Requirement::warning`] reports rules that are legal but almost
//! certainly a mistake.
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
//! The registry answers what a tree view needs: [`Unlocks::is_available`],
//! [`Unlocks::available`], [`Unlocks::dependencies`], and
//! [`Unlocks::rank`] for the column of a layout. [`Unlocks::validate`]
//! reports cycles and content nothing can reach.
//!
//! # Feature flags
//!
//! The crate is `no_std` and needs `alloc`. The `serde` feature derives
//! `Serialize` and `Deserialize` for [`Requirement`], [`CapabilitySet`],
//! [`Evaluation`], [`Unlock`], and [`Unlocks`], so rules are authorable in
//! JSON, RON, or YAML.

#![no_std]

extern crate alloc;

mod rules;
mod unlocks;

pub use rules::{CapabilitySet, Evaluation, Requirement};
pub use unlocks::{Unlock, UnlockWarning, Unlocks};

/// The README is compiled as part of the test suite, so its examples cannot rot.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
