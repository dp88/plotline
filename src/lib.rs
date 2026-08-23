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
//! # Example
//!
//! ```
//! use plotline::{Completion, Context, Flow, Library, Progress, Runner, Sequence,
//!                 Status, Step, TypeMap, steps};
//!
//! // A game-side step: waits for the world to signal. Timers, dialog boxes, and
//! // animations all look exactly like this from plotline's point of view.
//! struct WaitFor(Completion);
//!
//! impl Step for WaitFor {
//!     fn summary(&self) -> String { "Wait for the world".into() }
//!     fn flow(&self) -> Flow { Flow::Continue }
//!     fn start(&self, _ctx: &mut Context<'_>) -> Progress {
//!         Progress::Wait(self.0.clone())
//!     }
//! }
//!
//! let ready = Completion::new();
//!
//! let mut library = Library::new();
//! let farewell = library.insert(
//!     Sequence::new("farewell").with_step(steps::Log { message: "Safe roads.".into() }),
//! );
//! let greeting = library.insert(
//!     Sequence::new("greeting")
//!         .with_step(steps::Log { message: "Hello, traveler.".into() })
//!         .with_step(WaitFor(ready.clone()))
//!         .with_step(steps::Branch { condition: None, if_true: Some(farewell), if_false: None }),
//! );
//!
//! let mut runner = Runner::default();
//! let mut services = TypeMap::new();
//! runner.start(greeting, None).unwrap();
//!
//! // The chain holds at the wait...
//! assert_eq!(runner.advance(&mut library, &mut services), Status::Blocked);
//!
//! // ...until the world signals — from wherever, whenever "the world" is.
//! ready.signal();
//! assert_eq!(runner.advance(&mut library, &mut services), Status::Finished);
//! ```
//!
//! This crate requires `panic = "unwind"`: the runner isolates a panicking step
//! (log, skip, continue), which `panic = "abort"` would turn into process death.

#![warn(missing_docs)]
// A `match` over an Option whose arms both do real work reads better than `if let`,
// and every site clippy flags here logs on the missing side.
#![allow(clippy::single_match_else)]

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
pub use runner::{AbortReason, ChainGuard, Runner, RunnerConfig, RunnerEvent, StartError, Status};
pub use sequence::{Iter, Library, Sequence};
pub use source::{SequenceFacts, SequenceRef, SequenceSource};
pub use step::{Flow, IntoProgress, Progress, Step, StepFacts, StepRun};
pub use vocab::{Condition, Effect, EffectCtx, QueryCtx};

/// Compiles the README's code blocks as doctests, so the front page cannot go stale.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;
