//! A sequence of steps as a plain data structure: branching, subroutines, and external
//! waits — with no knowledge of clocks, frames, or engines.
//!
//! # The three ideas
//!
//! **A sequence is data.** [`Sequence`] is an ordered list of steps you build, iterate,
//! analyse ([`FlowModel`]), and run. Steps are shared, stateless config: every run
//! executes the same values, and everything per-run lives on the [`Runner`]. A
//! [`Library`] owns sequences and mints the [`SequenceRef`] handles they use to reference
//! each other.
//!
//! **The traits are the interconnect.** [`Step`] is how systems contribute verbs ("start
//! a battle", "open the shop"); [`Condition`] asks yes/no questions of the world;
//! [`Effect`] acts on it instantly. Conditions and effects are consumed far beyond the
//! runner — dialog choices, quest objectives, item use — which is why they, not the
//! runner, are the seam between systems.
//!
//! **Nothing here knows about time.** A step that cannot finish immediately returns a
//! [`Completion`] handle; *someone else* — a UI, an animation, a timer owned by the host
//! — signals it, and the host calls [`Runner::advance`] whenever that may have happened.
//! This crate is equally at home under a game loop, a CLI, or a unit test.
//!
//! If that shape feels familiar: this is a [`Poll`](core::task::Poll)-shaped pull loop
//! whose waker is your game loop. [`Runner::advance`] answers [`Poll::Pending`](core::task::Poll::Pending) while the
//! chain is in flight, and [`Progress`] is what one step answers with — finished,
//! waiting, or redirecting.
//!
//! # Writing a step
//!
//! Usually a closure. [`steps::run`] takes a name and a body, and the body answers with
//! whatever fits — nothing at all to finish now, a [`Completion`] to wait on, or a
//! [`Progress`] when it needs control flow:
//!
//! ```
//! use plotline::{Sequence, steps};
//!
//! let sequence = Sequence::new("greeting")
//!     .with_step(steps::run("Greet the elder", |_ctx| println!("Hello.")))
//!     .with_step(steps::run("Remember it", |ctx| ctx.set_flag("greeted", true)));
//! ```
//!
//! Implementing [`Step`] by hand is for steps that carry authored data — the ones an
//! editor writes and a save file holds.
//!
//! # Diagnostics
//!
//! This crate owns no logger, which is why it has no dependencies. The runner reports
//! what it did as [`RunnerEvent`]s; the host drains them with [`Runner::drain_events`]
//! and forwards them wherever it likes. Steps say things with
//! [`Context::note`]. A host that never drains sees nothing — that is the trade.
//!
//! # Example
//!
//! ```
//! use core::task::Poll;
//! use plotline::{Completion, Library, Outcome, Runner, Sequence, TypeMap, steps};
//!
//! // Something outside signals this: a dialog panel, an animation, a timer the host
//! // owns. From plotline's point of view they all look the same.
//! let ready = Completion::new();
//! let waiting_on = ready.clone();
//!
//! let mut library = Library::new();
//! let farewell = library.insert(
//!     Sequence::new("farewell").with_step(steps::run("Say goodbye", |_ctx| {
//!         println!("Safe roads.");
//!     })),
//! );
//! let greeting = library.insert(
//!     Sequence::new("greeting")
//!         .with_step(steps::run("Say hello", |_ctx| println!("Hello, traveler.")))
//!         // Returning a Completion means "wait on this".
//!         .with_step(steps::run("Wait for the world", move |_ctx| waiting_on.clone()))
//!         .with_step(steps::Branch {
//!             condition: None,
//!             if_true: Some(farewell),
//!             if_false: None,
//!         }),
//! );
//!
//! let mut runner = Runner::default();
//! let mut services = TypeMap::new();
//! runner.start(greeting, None).unwrap();
//!
//! // The chain holds at the wait...
//! assert_eq!(runner.advance(&mut library, &mut services), Poll::Pending);
//!
//! // ...until the world signals — from wherever, whenever "the world" is.
//! ready.signal();
//! assert_eq!(
//!     runner.advance(&mut library, &mut services),
//!     Poll::Ready(Outcome::Finished),
//! );
//! ```
//!
//! # Features
//!
//! `std` (default) buys **panic isolation**: the runner catches a panicking step, reports
//! it, and carries on. That needs an unwinder, so `panic = "abort"` turns the isolation
//! into process death — never set it in a profile that builds this crate.
//!
//! Without it the crate is `#![no_std]` (it still needs `alloc`) and makes the same
//! bargain `panic = "abort"` does: a panicking step takes the process with it, because
//! bare metal has no unwinder to offer.

#![warn(missing_docs)]
// A `match` over an Option whose arms both do real work reads better than `if let`,
// and every site clippy flags here logs on the missing side.
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
pub use sequence::{Iter, Library, Sequence};
pub use source::{SequenceFacts, SequenceRef, SequenceSource};
pub use step::{Flow, IntoProgress, Progress, Step, StepFacts, StepRun};
pub use vocab::{Condition, Effect, EffectCtx, QueryCtx};

/// Compiles the README's code blocks as doctests, so the front page cannot go stale.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;
