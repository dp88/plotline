//! Branching sequences with subroutines and external waits.
//!
//! [`Sequence`] stores shared steps. [`Runner`] stores run state. [`Library`] stores
//! sequences and creates their [`SequenceRef`] handles. Steps use [`Condition`] to read
//! state and [`Effect`] to change it.
//!
//! The crate does not own a clock or an engine. A waiting step returns [`Completion`].
//! The host signals it and calls [`Runner::advance`]. The runner reports diagnostics as
//! [`RunnerEvent`] values.
//!
//! The default `std` feature catches panics in steps. Without it, the crate uses `alloc`
//! and does not catch panics.

#![warn(missing_docs)]
#![allow(clippy::single_match_else)]
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

/// Compiles the README code blocks as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;
