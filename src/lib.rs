//! Branching sequences with subroutines and external waits.
//!
//! A sequence is an ordered list of steps:
//!
//! ```text
//! say "Hello, traveler."
//! say "Have you seen my ring?"
//! [choice] "Yes"  ──▶ ring_found
//!          "No"   ──▶ ring_lost
//!
//! ring_found:  remove item "gold ring"
//!              advance quest "The Lost Ring" to stage 2
//!              say "You have my thanks."
//! ```
//!
//! `plotline` runs order, branches, calls, returns, jumps, and waits. The
//! host defines the steps. Use it for dialog, quests, cutscenes, tutorials —
//! any authored flow that must not depend on an engine.
//!
//! It also answers the question that sits between such flows: does an entity
//! hold what a thing demands, and if not, what is it short of?
//!
//! ```text
//! Deneb IV requires
//!     ✓ FrozenHabitation
//!     ✗ HighGravityHabitation
//! ```
//!
//! # The pieces
//!
//! [`Sequence`] stores shared steps. [`Runner`] stores run state. [`Library`]
//! stores sequences and creates their [`SequenceRef`] handles. Steps use
//! [`Condition`] to read state and [`Effect`] to change it; both connect the
//! host systems and also work outside the runner.
//!
//! [`Requirement`] holds a boolean rule as data. [`CapabilitySet`] holds what
//! an entity has. [`Unlocks`] joins many of them into a graph. Together they
//! cover prerequisites, technology trees, permissions, and habitability
//! without the runner.
//!
//! # Control flow
//!
//! [`Progress::Call`] enters a subroutine, and falling off its end returns to
//! the caller. [`Progress::Return`] exits the current subroutine early.
//! [`Progress::Goto`] clears the whole call chain before starting its target,
//! or ends the chain when it has no target.
//!
//! # No clock
//!
//! A step that needs to wait returns [`Progress::Wait`] with a [`Completion`]
//! handle. The host signals the handle and calls [`Runner::advance`]. A
//! multi-phase step can instead return [`Progress::Resume`] and manage its
//! own per-run state through [`StepRun`]. The crate does not define timed
//! waits.
//!
//! # Requirements
//!
//! A [`Requirement`] is a tree over capability keys the host chooses. The
//! crate never interprets a key. [`Requirement::satisfies`] answers with a
//! bool and allocates nothing; [`Requirement::evaluate`] returns an
//! [`Evaluation`] tree that explains the answer, including the conservative
//! [`Evaluation::missing`] list. Both are pure functions of the requirement
//! and the [`CapabilitySet`], so they suit a user interface, a planner, or a
//! test.
//!
//! `Requirement` also implements [`Condition`], so a [`steps::Branch`] or
//! [`steps::when`] step can hold one. That path reads chain flags and the
//! [`conditions::Checks`] registry, so [`Requirement::Flag`] and
//! [`Requirement::Named`] work there. Call [`Requirement::satisfies_in`] or
//! [`Requirement::evaluate_in`] for the context answer, because the inherent
//! [`Requirement::evaluate`] shadows [`Condition::evaluate`].
//!
//! [`effects::grant`] and [`effects::revoke`] move a capability in or out of
//! the set. The evaluator does not care which source granted what.
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
//! The registry answers what a tree view needs: [`Unlocks::status`],
//! [`Unlocks::available`], [`Unlocks::dependencies`],
//! [`Unlocks::dependents`], and [`Unlocks::rank`] for the column of a layout.
//! [`Unlocks::validate`] reports cycles and content nothing can reach.
//!
//! # Closures and data
//!
//! A [`conditions::check`] closure reads any host state, but it cannot be
//! stored in a file or read by a tool. A `Requirement` can be both, but it
//! only asks about capabilities and flags. [`conditions::Checks`] joins them:
//! it gives a closure a name, and [`Requirement::Named`] calls that name.
//! [`Requirement::unknown_checks`] reports names no closure answers.
//!
//! # Built-ins
//!
//! The [`steps`], [`conditions`], and [`effects`] modules cover the common
//! cases. [`steps::run`] wraps a closure; its body can return `()`, a
//! [`Completion`], a [`Progress`], or any other type that implements
//! [`IntoProgress`]. [`conditions::check`] and [`effects::run`] give the
//! same closure-first style for conditions and effects, and [`steps::when`]
//! conditionally runs any step. Constructors such as [`effects::set_flag`],
//! [`steps::goto`], and [`steps::stop`] are shorthand over the public
//! structs and do not remove the struct-literal API.
//!
//! # Validation and analysis
//!
//! [`Library::validate`] reports empty or duplicate names, step and
//! nested-object warnings, and references to missing sequences. It permits
//! cycles, which are valid in authored graphs. [`StepFacts::references`]
//! exposes every outgoing sequence reference, including both sides of a
//! branch. [`FlowModel`] computes reachability for one sequence — the basis
//! for an editor's rail display.
//!
//! # Diagnostics
//!
//! The runner reports [`RunnerEvent`] values; the host drains them with
//! [`Runner::drain_events`] and decides how to log them. [`Context::note`]
//! adds location-tagged notes from inside a step. [`Condition::explain`]
//! returns an [`Explanation`] tree for any condition, including one whose
//! capability key type the caller does not know.
//!
//! # Feature flags
//!
//! The default `std` feature catches panics in steps, which requires
//! `panic = "unwind"`. Without it, the crate uses `alloc` only and does not
//! catch panics.
//!
//! The `serde` feature derives `Serialize` and `Deserialize` for
//! [`Requirement`], [`CapabilitySet`], and [`Evaluation`], so rules are
//! authorable in JSON, RON, or YAML. It works without `std`.

#![no_std]

extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

mod capability;
mod completion;
mod context;
mod evaluation;
mod flow_model;
mod requirement;
mod runner;
mod sequence;
mod source;
mod step;
mod unlocks;
mod vocab;

pub mod conditions;
pub mod effects;
pub mod steps;

pub use capability::CapabilitySet;
pub use completion::Completion;
pub use context::{ChainFlags, Context, TypeMap};
pub use evaluation::Evaluation;
pub use flow_model::{FlowModel, RailNode, RailShape};
pub use requirement::{Nothing, Requirement, Rule};
pub use runner::{
    AbortReason, ChainGuard, Outcome, Runner, RunnerConfig, RunnerEvent, SkipReason, StartError,
};
pub use sequence::{Iter, Library, Sequence, ValidationWarning};
pub use source::{SequenceFacts, SequenceRef, SequenceSource};
pub use step::{Flow, IntoProgress, Progress, Step, StepFacts, StepRun};
pub use unlocks::{Status, Unlock, UnlockWarning, Unlocks};
pub use vocab::{Condition, Effect, EffectCtx, Explanation, QueryCtx};

/// The README is compiled as part of the test suite, so its examples cannot rot.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
