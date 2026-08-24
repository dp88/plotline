//! Step execution types.

#[cfg(feature = "std")]
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::completion::Completion;
use crate::context::Context;
use crate::source::SequenceRef;

/// Whether execution continues after a step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// The next step runs.
    #[default]
    Continue,
    /// No later step runs.
    End,
}

/// Result of one step execution.
pub enum Progress {
    /// The step finished.
    Done,
    /// The runner waits until the handle is signaled.
    Wait(Completion),
    /// Runs another sequence as a subroutine.
    Call(SequenceRef),
    /// Returns from the current subroutine, or finishes the chain at the root.
    Return,
    /// Ends the current chain and optionally starts `target`.
    Goto(Option<SequenceRef>),
    /// Returns a per-run state machine.
    Resume(Box<dyn StepRun>),
}

impl core::fmt::Debug for Progress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Done => f.write_str("Done"),
            Self::Wait(c) => f.debug_tuple("Wait").field(c).finish(),
            Self::Call(r) => f.debug_tuple("Call").field(r).finish(),
            Self::Return => f.write_str("Return"),
            Self::Goto(r) => f.debug_tuple("Goto").field(r).finish(),
            Self::Resume(_) => f.write_str("Resume(..)"),
        }
    }
}

/// Converts common closure results to [`Progress`].
pub trait IntoProgress {
    /// Converts the value.
    fn into_progress(self) -> Progress;
}

impl IntoProgress for () {
    fn into_progress(self) -> Progress {
        Progress::Done
    }
}

impl IntoProgress for Progress {
    fn into_progress(self) -> Progress {
        self
    }
}

impl IntoProgress for Completion {
    fn into_progress(self) -> Progress {
        Progress::Wait(self)
    }
}

/// Per-run state for a multi-phase step.
pub trait StepRun {
    /// Resumes the step.
    fn resume(&mut self, ctx: &mut Context<'_>) -> Progress;
}

/// Shared configuration and execution for one sequence step.
pub trait Step {
    /// Returns a display summary.
    fn summary(&self) -> String;

    /// Returns an authoring warning, if any.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Returns the declared flow. The default is [`Flow::Continue`].
    fn flow(&self) -> Flow {
        Flow::Continue
    }

    /// Returns the delegated sequence, if any.
    fn delegates_to(&self) -> Option<SequenceRef> {
        None
    }

    /// Returns every sequence referenced by this step.
    fn references(&self) -> Vec<SequenceRef> {
        self.delegates_to().into_iter().collect()
    }

    /// Returns whether the runner can execute this step.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Starts the step.
    fn start(&self, ctx: &mut Context<'_>) -> Progress;
}

/// Snapshot of a step's reported facts.
#[derive(Clone, Debug)]
pub struct StepFacts {
    /// The step summary, or a panic placeholder.
    pub summary: String,
    /// The step warning, or a panic report.
    pub warning: Option<String>,
    /// The step flow, or [`Flow::Continue`] after a panic.
    pub flow: Flow,
    /// The delegated sequence.
    pub delegates_to: Option<SequenceRef>,
    /// Every sequence referenced by the step.
    pub references: Vec<SequenceRef>,
    /// Whether the step is enabled.
    pub enabled: bool,
}

impl StepFacts {
    /// Reads only whether a step is enabled, isolating panics when `std` is enabled.
    pub(crate) fn enabled_of(step: &dyn Step) -> bool {
        #[cfg(feature = "std")]
        {
            catch_unwind(AssertUnwindSafe(|| step.is_enabled())).unwrap_or(true)
        }
        #[cfg(not(feature = "std"))]
        step.is_enabled()
    }

    /// Collects a step's facts.
    #[must_use]
    pub fn of(step: &dyn Step) -> Self {
        #[cfg(feature = "std")]
        {
            catch_unwind(AssertUnwindSafe(|| Self::gather(step))).unwrap_or_else(|_| Self {
                summary: "<step panicked describing itself>".to_owned(),
                warning: Some("This step panicked while describing itself.".to_owned()),
                flow: Flow::Continue,
                delegates_to: None,
                references: Vec::new(),
                enabled: true,
            })
        }
        #[cfg(not(feature = "std"))]
        Self::gather(step)
    }

    fn gather(step: &dyn Step) -> Self {
        Self {
            summary: step.summary(),
            warning: step.warning(),
            flow: step.flow(),
            delegates_to: step.delegates_to(),
            references: step.references(),
            enabled: step.is_enabled(),
        }
    }
}

#[cfg(test)]
mod tests {

    use alloc::boxed::Box;

    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    struct Fine;
    impl Step for Fine {
        fn summary(&self) -> String {
            "fine".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Done
        }
    }

    #[cfg(feature = "std")]
    struct PanicsDescribing;
    #[cfg(feature = "std")]
    impl Step for PanicsDescribing {
        fn summary(&self) -> String {
            panic!("half-authored")
        }
        fn flow(&self) -> Flow {
            Flow::End
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Done
        }
    }

    #[test]
    fn step_is_object_safe() {
        let steps: Vec<Box<dyn Step>> = vec![Box::new(Fine)];
        assert_eq!(steps[0].summary(), "fine");
        assert_eq!(steps[0].flow(), Flow::Continue);
        assert!(steps[0].is_enabled());
        assert!(steps[0].warning().is_none());
        assert!(steps[0].delegates_to().is_none());
        assert!(steps[0].references().is_empty());
    }

    #[test]
    fn facts_snapshot_a_well_behaved_step() {
        let facts = StepFacts::of(&Fine);
        assert_eq!(facts.summary, "fine");
        assert_eq!(facts.flow, Flow::Continue);
        assert!(facts.references.is_empty());
        assert!(facts.enabled);
    }

    #[test]
    #[cfg(feature = "std")] // panic isolation needs an unwinder
    fn facts_survive_a_panicking_accessor() {
        let facts = StepFacts::of(&PanicsDescribing);
        assert!(facts.warning.is_some());
        assert_eq!(facts.flow, Flow::Continue);
    }
}
