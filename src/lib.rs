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
//! # The pieces
//!
//! [`Sequence`] stores shared steps. [`Runner`] stores run state. [`Library`]
//! stores sequences and creates their [`SequenceRef`] handles. Steps use
//! [`Condition`] to read state and [`Effect`] to change it; both connect the
//! host systems and also work outside the runner.
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
//! # Built-ins
//!
//! The [`steps`], [`conditions`], and [`effects`] modules cover the common
//! cases. [`steps::run`] wraps a closure; its body can return `()`, a
//! [`Completion`], a [`Progress`], or any other type that implements
//! [`IntoProgress`]. [`conditions::check`] and [`effects::run`] give the
//! same closure-first style for conditions and effects, and [`steps::when`]
//! conditionally runs any step. Constructors such as [`conditions::flag`],
//! [`effects::set_flag`], [`steps::goto`], and [`steps::stop`] are shorthand
//! over the public structs and do not remove the struct-literal API.
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
//! adds location-tagged notes from inside a step.
//!
//! # Feature flags
//!
//! The default `std` feature catches panics in steps, which requires
//! `panic = "unwind"`. Without it, the crate uses `alloc` only and does not
//! catch panics.

#![no_std]

extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

mod completion;
mod context;
mod flow_model;
mod runner;
mod sequence;
mod source;
mod step;
mod vocab;

pub mod conditions;
pub mod effects;
pub mod steps;

pub use completion::Completion;
pub use context::{ChainFlags, Context, TypeMap};
pub use flow_model::{FlowModel, RailNode, RailShape};
pub use runner::{
    AbortReason, ChainGuard, Outcome, Runner, RunnerConfig, RunnerEvent, SkipReason, StartError,
};
pub use sequence::{Iter, Library, Sequence, ValidationWarning};
pub use source::{SequenceFacts, SequenceRef, SequenceSource};
pub use step::{Flow, IntoProgress, Progress, Step, StepFacts, StepRun};
pub use vocab::{Condition, Effect, EffectCtx, QueryCtx};

/// The README is compiled as part of the test suite, so its examples cannot rot.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
