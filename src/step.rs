//! The step contract: what a sequence is made of.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::completion::Completion;
use crate::context::Context;
use crate::source::SequenceRef;

/// Whether execution continues past a step.
///
/// Self-reported by the step. There is no "might end" answer: a step that hands control
/// to another sequence says so through [`Step::delegates_to`], and analysis resolves what
/// that contributes. One fact, stated once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// The next step runs after this one.
    #[default]
    Continue,
    /// No step after this one ever runs.
    End,
}

/// How one execution of a step progressed.
///
/// A step either finishes during its call, or returns *why it isn't finished* as a plain
/// value. There is no clock and no frame in this vocabulary — waiting is only ever "this
/// handle hasn't been signaled", and whoever signals it (a UI, a timer owned by the host,
/// a test) is outside the crate's knowledge.
pub enum Progress {
    /// The step finished during this call.
    Done,
    /// The step is waiting on the outside world; the runner holds here until the handle
    /// is signaled.
    Wait(Completion),
    /// Run another sequence inline against the same context (a subroutine), then this
    /// step is done.
    Call(SequenceRef),
    /// End this sequence — and every subroutine above it — then continue the chain at
    /// `target`, or end the chain when `target` is `None`.
    ///
    /// This is the only way a step redirects the chain. It is a return value rather than
    /// a method on [`Context`] so that control flow is visible in the signature, and so
    /// that a step can be tested by what it answers.
    Goto(Option<SequenceRef>),
    /// The step has more to do: an explicit state machine the runner owns and resumes.
    /// The box is the per-run state — created fresh each execution, dropped after —
    /// which is what keeps the [`Step`] itself shared, stateless config.
    Resume(Box<dyn StepRun>),
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Done => f.write_str("Done"),
            Self::Wait(c) => f.debug_tuple("Wait").field(c).finish(),
            Self::Call(r) => f.debug_tuple("Call").field(r).finish(),
            Self::Goto(r) => f.debug_tuple("Goto").field(r).finish(),
            Self::Resume(_) => f.write_str("Resume(..)"),
        }
    }
}

/// The per-run state machine of a multi-phase step.
///
/// Most steps never need this: they finish in one call, or wait once and are done. A step
/// with several phases — show a line, wait, show the next line — returns
/// [`Progress::Resume`] from [`Step::start`], and the runner calls [`resume`] again each
/// time the previously returned [`Wait`](Progress::Wait) or [`Call`](Progress::Call)
/// resolves, until it answers [`Done`](Progress::Done).
///
/// Implementations must not panic in `Drop`: an abandoned run is dropped during unwind
/// when a chain stops, and a panicking destructor there would abort the process.
///
/// [`resume`]: StepRun::resume
pub trait StepRun {
    /// Carries the step to its next wait or to completion.
    fn resume(&mut self, ctx: &mut Context<'_>) -> Progress;
}

/// One step of a sequence: shared, stateless config that knows how to describe itself and
/// how to execute once.
///
/// Every run of a sequence executes the same step values, so a step must hold no per-run
/// state — anything per-run lives in the [`Progress`] it returns. All methods take
/// `&self` for the same reason, and because storage adapters may expose steps through
/// shared borrows.
pub trait Step {
    /// One line describing this instance for lists and logs — the action with its data
    /// ("Wait 2.50s"), not the type name.
    fn summary(&self) -> String;

    /// One line naming an authoring problem, or `None` when the configuration is fine.
    /// The step is the one authority on what a valid configuration of itself looks like;
    /// editors and analysis surface whatever it reports.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Whether execution continues past this step. Defaults to [`Flow::Continue`],
    /// which is what all but control-flow steps answer.
    fn flow(&self) -> Flow {
        Flow::Continue
    }

    /// The sequence this step hands control to. A step that answers `Some` *may* end the
    /// sequence, depending on what the target contains; analysis chases that through.
    /// The runner never reads this.
    fn delegates_to(&self) -> Option<SequenceRef> {
        None
    }

    /// Soft-delete toggle: the runner skips disabled steps entirely, and flow analysis
    /// treats them as [`Flow::Continue`] (a step that never runs cannot end anything).
    fn is_enabled(&self) -> bool {
        true
    }

    /// Executes the step. Communicate only through `ctx`: a step that needs a system
    /// reaches it via the context's services, and a missing service must log an error
    /// and finish rather than wait for something that will never come.
    fn start(&self, ctx: &mut Context<'_>) -> Progress;
}

/// A snapshot of one step's self-reported facts, so analysis and editors never hold a
/// borrow into step storage.
///
/// Gather it with [`StepFacts::of`], which shields the caller from a panicking accessor —
/// authored content is allowed to be half-finished, and a `summary()` that panics on it
/// must not take an inspector down.
#[derive(Clone, Debug)]
pub struct StepFacts {
    /// [`Step::summary`], or a placeholder when the step panicked describing itself.
    pub summary: String,
    /// [`Step::warning`], or a report of the panic when describing failed.
    pub warning: Option<String>,
    /// [`Step::flow`], or [`Flow::Continue`] when describing failed (the conservative
    /// answer: an undescribed step severs nothing).
    pub flow: Flow,
    /// [`Step::delegates_to`].
    pub delegates_to: Option<SequenceRef>,
    /// [`Step::is_enabled`].
    pub enabled: bool,
}

impl StepFacts {
    /// Gathers the facts, converting a panic in any accessor into a warning fact instead
    /// of propagating it.
    #[must_use]
    pub fn of(step: &dyn Step) -> Self {
        catch_unwind(AssertUnwindSafe(|| Self {
            summary: step.summary(),
            warning: step.warning(),
            flow: step.flow(),
            delegates_to: step.delegates_to(),
            enabled: step.is_enabled(),
        }))
        .unwrap_or_else(|_| Self {
            summary: "<step panicked describing itself>".to_owned(),
            warning: Some("This step panicked while describing itself.".to_owned()),
            flow: Flow::Continue,
            delegates_to: None,
            enabled: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    struct PanicsDescribing;
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
    }

    #[test]
    fn facts_snapshot_a_well_behaved_step() {
        let facts = StepFacts::of(&Fine);
        assert_eq!(facts.summary, "fine");
        assert_eq!(facts.flow, Flow::Continue);
        assert!(facts.enabled);
    }

    #[test]
    fn facts_survive_a_panicking_accessor() {
        // "Authored content is allowed to be half-finished; a property that throws on it
        // must not take the inspector down with it."
        let facts = StepFacts::of(&PanicsDescribing);
        assert!(facts.warning.is_some());
        assert_eq!(facts.flow, Flow::Continue);
    }
}
