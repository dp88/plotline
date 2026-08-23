//! The sequence runner.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use alloc::collections::VecDeque;
use core::any::Any;
#[cfg(feature = "std")]
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The payload a panicking step left behind.
type Panic = Box<dyn Any + Send>;

/// Runs a body with optional panic isolation.
#[cfg(feature = "std")]
fn isolate<T>(body: impl FnOnce() -> T) -> Result<T, Panic> {
    catch_unwind(AssertUnwindSafe(body))
}

#[cfg(not(feature = "std"))]
#[allow(clippy::unnecessary_wraps)]
fn isolate<T>(body: impl FnOnce() -> T) -> Result<T, Panic> {
    Ok(body())
}

use crate::context::{ChainState, Context, TypeMap};
use crate::source::{SequenceRef, SequenceSource};
use core::task::Poll;

use crate::step::{Progress, StepRun};

/// Limits for one runner.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RunnerConfig {
    /// Maximum active sequence frames.
    pub max_call_depth: u32,
    /// Maximum sequence hops per [`Runner::advance`].
    pub max_hops_per_advance: u32,
    /// Maximum resumes per [`Runner::advance`].
    pub max_resumes_per_advance: u32,
    /// Maximum buffered [`RunnerEvent`] values.
    pub max_buffered_events: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_call_depth: 32,
            max_hops_per_advance: 64,
            max_resumes_per_advance: 256,
            max_buffered_events: 4096,
        }
    }
}

impl RunnerConfig {
    /// Sets [`RunnerConfig::max_call_depth`].
    #[must_use]
    pub const fn max_call_depth(mut self, frames: u32) -> Self {
        self.max_call_depth = frames;
        self
    }

    /// Sets [`RunnerConfig::max_hops_per_advance`].
    #[must_use]
    pub const fn max_hops_per_advance(mut self, hops: u32) -> Self {
        self.max_hops_per_advance = hops;
        self
    }

    /// Sets [`RunnerConfig::max_resumes_per_advance`].
    #[must_use]
    pub const fn max_resumes_per_advance(mut self, resumes: u32) -> Self {
        self.max_resumes_per_advance = resumes;
        self
    }

    /// Sets [`RunnerConfig::max_buffered_events`].
    #[must_use]
    pub const fn max_buffered_events(mut self, events: usize) -> Self {
        self.max_buffered_events = events;
        self
    }
}

/// Result of [`Runner::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// No chain was active.
    Idle,
    /// The chain finished.
    Finished,
    /// A guard stopped the chain.
    Aborted(AbortReason),
}

/// Why a chain was aborted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AbortReason {
    /// The hop limit was reached.
    HopLimit,
    /// The resume limit was reached.
    ResumeLimit,
    /// The call-depth limit was reached.
    CallDepth,
}

/// Why a step was skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// The step was disabled.
    Disabled,
    /// The source had no step at the index.
    Missing,
    /// The step was removed after inspection.
    Vanished,
}

/// Diagnostic event emitted by the runner.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunnerEvent {
    /// An enabled step started.
    StepStarted {
        /// Sequence handle.
        sequence: SequenceRef,
        /// Step index.
        index: usize,
    },
    /// A step panicked and was skipped.
    StepFailed {
        /// Sequence handle.
        sequence: SequenceRef,
        /// Step index.
        index: usize,
        /// Panic message.
        reason: String,
    },
    /// A step was skipped.
    StepSkipped {
        /// Sequence handle.
        sequence: SequenceRef,
        /// Step index.
        index: usize,
        /// Skip reason.
        why: SkipReason,
    },
    /// A sequence handle did not resolve.
    SequenceMissing {
        /// Missing handle.
        sequence: SequenceRef,
    },
    /// A step emitted a note.
    Note {
        /// Sequence handle.
        sequence: SequenceRef,
        /// Step index.
        index: usize,
        /// Note text.
        message: String,
    },
}

/// Bounded event buffer.
#[derive(Debug, Default)]
pub(crate) struct Events {
    queue: VecDeque<RunnerEvent>,
    cap: usize,
}

impl Events {
    /// Records an event.
    pub(crate) fn record(&mut self, event: RunnerEvent) {
        if self.queue.len() >= self.cap {
            self.queue.pop_front();
        }
        self.queue.push_back(event);
    }
}

/// Why [`Runner::start`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartError {
    /// A chain is already active.
    AlreadyRunning,
}

impl core::fmt::Display for StartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("a sequence chain is already running"),
        }
    }
}

impl core::error::Error for StartError {}

/// Host-owned value held for the life of a chain.
pub type ChainGuard = Box<dyn Any>;

struct Frame {
    sequence: SequenceRef,
    index: usize,
    run: Option<Box<dyn StepRun>>,
}

impl Frame {
    fn new(sequence: SequenceRef) -> Self {
        Self {
            sequence,
            index: 0,
            run: None,
        }
    }

    fn next(&mut self) {
        self.run = None;
        self.index += 1;
    }
}

struct Chain {
    state: ChainState,
    frames: Vec<Frame>,
    pending: Option<crate::Completion>,
    guard: Option<ChainGuard>,
}

enum Drive {
    Blocked,
    Finished,
    Aborted(AbortReason),
}

/// Runs one chain at a time.
///
/// ```
/// use core::task::Poll;
/// use plotline::{Library, Outcome, Runner, Sequence, TypeMap, steps};
///
/// let mut library = Library::new();
/// let sequence = library.insert(
///     Sequence::new("hello").with_step(steps::run("Greet", |_ctx| {})),
/// );
/// let mut runner = Runner::default();
/// let mut services = TypeMap::new();
/// runner.start(sequence, None).unwrap();
/// assert_eq!(
///     runner.advance(&mut library, &mut services),
///     Poll::Ready(Outcome::Finished),
/// );
/// ```
pub struct Runner {
    config: RunnerConfig,
    guard_factory: Option<Box<dyn FnMut() -> ChainGuard>>,
    chain: Option<Chain>,
    events: Events,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new(RunnerConfig::default())
    }
}

impl Runner {
    /// Creates an idle runner with the given limits.
    #[must_use]
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            config,
            guard_factory: None,
            chain: None,
            events: Events::default(),
        }
    }

    /// Installs the guard factory.
    pub fn set_guard_factory(&mut self, factory: impl FnMut() -> ChainGuard + 'static) {
        self.guard_factory = Some(Box::new(factory));
    }

    /// Returns mutable runner limits.
    pub fn config_mut(&mut self) -> &mut RunnerConfig {
        &mut self.config
    }

    /// Starts a chain. Execution begins with [`Runner::advance`].
    ///
    /// # Errors
    ///
    /// Returns [`StartError::AlreadyRunning`] if a chain is active.
    pub fn start(
        &mut self,
        sequence: SequenceRef,
        instigator: Option<Box<dyn Any>>,
    ) -> Result<(), StartError> {
        if self.chain.is_some() {
            return Err(StartError::AlreadyRunning);
        }
        self.chain = Some(Chain {
            state: ChainState {
                instigator,
                ..ChainState::default()
            },
            frames: vec![Frame::new(sequence)],
            pending: None,
            guard: self.guard_factory.as_mut().map(|factory| factory()),
        });
        Ok(())
    }

    /// Advances the chain until it waits or ends.
    pub fn advance(
        &mut self,
        source: &mut dyn SequenceSource,
        services: &mut TypeMap,
    ) -> Poll<Outcome> {
        self.events.cap = self.config.max_buffered_events;
        let Some(mut chain) = self.chain.take() else {
            return Poll::Ready(Outcome::Idle);
        };
        let outcome =
            match Self::drive(&mut chain, &self.config, &mut self.events, source, services) {
                Drive::Blocked => {
                    self.chain = Some(chain);
                    return Poll::Pending;
                }
                Drive::Finished => Outcome::Finished,
                Drive::Aborted(reason) => Outcome::Aborted(reason),
            };
        drop(chain.guard.take());
        Poll::Ready(outcome)
    }

    /// Stops the active chain and returns whether one existed.
    pub fn stop(&mut self) -> bool {
        match self.chain.take() {
            Some(mut chain) => {
                drop(chain.guard.take());
                true
            }
            None => false,
        }
    }

    /// Returns whether a chain is active.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.chain.is_some()
    }

    /// Returns the current sequence and step index.
    #[must_use]
    pub fn current(&self) -> Option<(SequenceRef, usize)> {
        let frame = self.chain.as_ref()?.frames.last()?;
        Some((frame.sequence, frame.index))
    }

    /// Drains buffered events in order.
    pub fn drain_events(&mut self) -> impl Iterator<Item = RunnerEvent> + '_ {
        self.events.queue.drain(..)
    }

    #[allow(clippy::too_many_lines)]
    fn drive(
        chain: &mut Chain,
        config: &RunnerConfig,
        events: &mut Events,
        source: &mut dyn SequenceSource,
        services: &mut TypeMap,
    ) -> Drive {
        let mut hops: u32 = 0;
        let mut resumes: u32 = 0;
        let mut resume_current = false;

        loop {
            if let Some(pending) = &chain.pending {
                if !pending.is_complete() {
                    return Drive::Blocked;
                }
                chain.pending = None;
                resume_current = true;
            }

            if resume_current {
                resume_current = false;
                let has_machine = Self::top(chain).run.is_some();
                if has_machine {
                    let (sequence, index) = {
                        let frame = Self::top(chain);
                        (frame.sequence, frame.index)
                    };
                    resumes += 1;
                    if resumes > config.max_resumes_per_advance {
                        return Drive::Aborted(AbortReason::ResumeLimit);
                    }
                    let caught = {
                        let Chain { state, frames, .. } = &mut *chain;
                        let machine = frames
                            .last_mut()
                            .expect("a chain always has a frame")
                            .run
                            .as_mut()
                            .expect("checked above");
                        let mut ctx = Context::new(services, state, events, (sequence, index));
                        isolate(|| machine.resume(&mut ctx))
                    };
                    match caught {
                        Ok(progress) => {
                            if let Some(drive) = Self::apply_progress(
                                chain,
                                config,
                                progress,
                                &mut resume_current,
                                &mut hops,
                            ) {
                                return drive;
                            }
                        }
                        Err(payload) => {
                            Self::fail_step(chain, events, sequence, index, &*payload);
                        }
                    }
                    continue;
                }
                Self::top(chain).next();
            }

            let (sequence, index) = {
                let frame = Self::top(chain);
                (frame.sequence, frame.index)
            };
            let ended = match source.step_count(sequence) {
                Some(count) => index >= count,
                None => {
                    events.record(RunnerEvent::SequenceMissing { sequence });
                    true
                }
            };
            if ended {
                if chain.frames.len() > 1 {
                    chain.frames.pop();
                    resume_current = true;
                    continue;
                }
                return Drive::Finished;
            }

            match source.step_facts(sequence, index) {
                None => {
                    Self::skip(events, sequence, index, SkipReason::Missing);
                    Self::top(chain).next();
                }
                Some(facts) if !facts.enabled => {
                    Self::skip(events, sequence, index, SkipReason::Disabled);
                    Self::top(chain).next();
                }
                Some(_facts) => {
                    events.record(RunnerEvent::StepStarted { sequence, index });
                    let caught = {
                        let Chain { state, .. } = &mut *chain;
                        let mut ctx = Context::new(services, state, events, (sequence, index));
                        isolate(|| source.start_step(sequence, index, &mut ctx))
                    };
                    match caught {
                        Ok(Some(progress)) => {
                            if let Some(drive) = Self::apply_progress(
                                chain,
                                config,
                                progress,
                                &mut resume_current,
                                &mut hops,
                            ) {
                                return drive;
                            }
                        }
                        Ok(None) => {
                            Self::skip(events, sequence, index, SkipReason::Vanished);
                            Self::top(chain).next();
                        }
                        Err(payload) => {
                            Self::fail_step(chain, events, sequence, index, &*payload);
                        }
                    }
                }
            }
        }
    }

    fn top(chain: &mut Chain) -> &mut Frame {
        chain.frames.last_mut().expect("a chain always has a frame")
    }

    fn skip(events: &mut Events, sequence: SequenceRef, index: usize, why: SkipReason) {
        events.record(RunnerEvent::StepSkipped {
            sequence,
            index,
            why,
        });
    }

    fn apply_progress(
        chain: &mut Chain,
        config: &RunnerConfig,
        progress: Progress,
        resume_current: &mut bool,
        hops: &mut u32,
    ) -> Option<Drive> {
        match progress {
            Progress::Done => Self::top(chain).next(),
            Progress::Wait(completion) => {
                chain.pending = Some(completion);
            }
            Progress::Call(target) => {
                if chain.frames.len() >= config.max_call_depth as usize {
                    return Some(Drive::Aborted(AbortReason::CallDepth));
                }
                chain.frames.push(Frame::new(target));
            }
            Progress::Goto(target) => {
                let Some(next) = target else {
                    return Some(Drive::Finished);
                };
                *hops += 1;
                if *hops > config.max_hops_per_advance {
                    return Some(Drive::Aborted(AbortReason::HopLimit));
                }
                chain.frames.clear();
                chain.frames.push(Frame::new(next));
                *resume_current = false;
            }
            Progress::Resume(machine) => {
                Self::top(chain).run = Some(machine);
                *resume_current = true;
            }
        }
        None
    }

    fn fail_step(
        chain: &mut Chain,
        events: &mut Events,
        sequence: SequenceRef,
        index: usize,
        payload: &(dyn Any + Send),
    ) {
        let reason = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        events.record(RunnerEvent::StepFailed {
            sequence,
            index,
            reason,
        });
        Self::top(chain).next();
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;
    use alloc::format;
    use alloc::rc::Rc;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::completion::Completion;
    use crate::sequence::{Library, Sequence};
    use crate::source::SequenceFacts;
    use crate::step::{Flow, Step, StepFacts};
    use crate::steps;

    /// Records execution for a test.
    struct Probe {
        label: &'static str,
        seen: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Step for Probe {
        fn summary(&self) -> String {
            format!("Probe {}", self.label)
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            self.seen.borrow_mut().push(self.label);
            Progress::Done
        }
    }

    /// Disabled test step.
    struct DisabledProbe(Probe);

    impl Step for DisabledProbe {
        fn summary(&self) -> String {
            self.0.summary()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn is_enabled(&self) -> bool {
            false
        }
        fn start(&self, ctx: &mut Context<'_>) -> Progress {
            self.0.start(ctx)
        }
    }

    /// Waits on a completion.
    struct WaitOnce(Completion);

    impl Step for WaitOnce {
        fn summary(&self) -> String {
            "Wait once".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Wait(self.0.clone())
        }
    }

    /// Panics when run.
    #[cfg(feature = "std")]
    struct Panics;

    #[cfg(feature = "std")]
    impl Step for Panics {
        fn summary(&self) -> String {
            "Panics".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            panic!("boom")
        }
    }

    /// Two-phase test step.
    struct TwoPhase {
        first: Completion,
        second: Completion,
        resumes: Rc<Cell<usize>>,
    }

    struct TwoPhaseRun {
        phase: usize,
        first: Completion,
        second: Completion,
        resumes: Rc<Cell<usize>>,
    }

    impl Step for TwoPhase {
        fn summary(&self) -> String {
            "Two phases".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Resume(Box::new(TwoPhaseRun {
                phase: 0,
                first: self.first.clone(),
                second: self.second.clone(),
                resumes: self.resumes.clone(),
            }))
        }
    }

    impl StepRun for TwoPhaseRun {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            self.resumes.set(self.resumes.get() + 1);
            self.phase += 1;
            match self.phase {
                1 => Progress::Wait(self.first.clone()),
                2 => Progress::Wait(self.second.clone()),
                _ => Progress::Done,
            }
        }
    }

    /// Calls a sequence, then finishes.
    struct CallThenDone {
        target: SequenceRef,
        resumes: Rc<Cell<usize>>,
    }

    struct CallThenDoneRun {
        target: SequenceRef,
        called: bool,
        resumes: Rc<Cell<usize>>,
    }

    impl Step for CallThenDone {
        fn summary(&self) -> String {
            "Call, then finish".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Resume(Box::new(CallThenDoneRun {
                target: self.target,
                called: false,
                resumes: self.resumes.clone(),
            }))
        }
    }

    impl StepRun for CallThenDoneRun {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            self.resumes.set(self.resumes.get() + 1);
            if self.called {
                Progress::Done
            } else {
                self.called = true;
                Progress::Call(self.target)
            }
        }
    }

    /// Checks the instigator type and value.
    struct SeesInstigator {
        expected: u32,
        seen: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Step for SeesInstigator {
        fn summary(&self) -> String {
            "Sees instigator".into()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, ctx: &mut Context<'_>) -> Progress {
            if ctx.instigator_as::<u32>() == Some(&self.expected) {
                self.seen.borrow_mut().push("instigator-ok");
            }
            Progress::Done
        }
    }

    /// Sets a flag when dropped.
    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    fn seen() -> Rc<RefCell<Vec<&'static str>>> {
        Rc::new(RefCell::new(Vec::new()))
    }

    fn probe(label: &'static str, seen: &Rc<RefCell<Vec<&'static str>>>) -> Probe {
        Probe {
            label,
            seen: seen.clone(),
        }
    }

    fn advance(runner: &mut Runner, library: &mut Library) -> Poll<Outcome> {
        let mut services = TypeMap::new();
        runner.advance(library, &mut services)
    }

    #[test]
    fn finishes_an_empty_sequence() {
        let mut library = Library::new();
        let empty = library.insert(Sequence::new("empty"));
        let mut runner = Runner::default();
        runner.start(empty, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert!(!runner.is_running());
    }

    #[test]
    fn runs_steps_in_order() {
        let seen = seen();
        let mut library = Library::new();
        let s = library.insert(
            Sequence::new("s")
                .with_step(probe("a", &seen))
                .with_step(probe("b", &seen))
                .with_step(probe("c", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(s, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn trampoline_runs_flat_across_branches() {
        let seen = seen();
        let mut library = Library::new();
        let c = library.insert(Sequence::new("c").with_step(probe("c", &seen)));
        let b = library.insert(Sequence::new("b").with_step(probe("b", &seen)).with_step(
            steps::Branch {
                condition: None,
                if_true: Some(c),
                if_false: None,
            },
        ));
        let a = library.insert(Sequence::new("a").with_step(probe("a", &seen)).with_step(
            steps::Branch {
                condition: None,
                if_true: Some(b),
                if_false: None,
            },
        ));
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn take_next_rearms_stopped_for_the_successor() {
        let seen = seen();
        let mut library = Library::new();
        let b = library.insert(Sequence::new("b").with_step(probe("b", &seen)));
        let a = library.insert(Sequence::new("a").with_step(steps::Branch {
            condition: None,
            if_true: Some(b),
            if_false: None,
        }));
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["b"]);
    }

    #[test]
    fn branch_to_none_ends_the_chain() {
        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Branch::default()) // no condition, both targets None
                .with_step(probe("unreachable", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn subroutine_shares_context_so_a_nested_stop_ends_the_caller() {
        let seen = seen();
        let mut library = Library::new();
        let sub = library.insert(Sequence::new("sub").with_step(steps::Stop));
        let a = library.insert(
            Sequence::new("a")
                .with_step(probe("before", &seen))
                .with_step(steps::Call {
                    sequence: Some(sub),
                })
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(
            *seen.borrow(),
            vec!["before"],
            "the nested Stop ended the caller"
        );
    }

    #[test]
    fn subroutine_flags_survive_into_the_caller() {
        let seen = seen();
        let mut library = Library::new();
        let happy = library.insert(Sequence::new("happy").with_step(probe("happy", &seen)));
        let sub = library.insert(Sequence::new("sub").with_step(steps::SetFlag {
            name: "accepted".into(),
            value: true,
        }));
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Call {
                    sequence: Some(sub),
                })
                .with_step(steps::Branch {
                    condition: Some(Box::new(crate::conditions::Flag::is_set("accepted"))),
                    if_true: Some(happy),
                    if_false: None,
                }),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["happy"]);
    }

    #[test]
    fn nested_branch_replaces_the_callers_chain() {
        let seen = seen();
        let mut library = Library::new();
        let c = library.insert(Sequence::new("c").with_step(probe("c", &seen)));
        let sub = library.insert(Sequence::new("sub").with_step(steps::Branch {
            condition: None,
            if_true: Some(c),
            if_false: None,
        }));
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Call {
                    sequence: Some(sub),
                })
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(
            *seen.borrow(),
            vec!["c"],
            "the caller's remaining steps are severed by the nested branch"
        );
    }

    #[test]
    fn call_depth_overflow_aborts() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a"));
        library
            .get_mut(a)
            .unwrap()
            .push(steps::Call { sequence: Some(a) });
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Aborted(AbortReason::CallDepth))
        );
    }

    #[test]
    fn hop_limit_aborts_an_authored_loop() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a"));
        library.get_mut(a).unwrap().push(steps::Branch {
            condition: None,
            if_true: Some(a),
            if_false: None,
        });
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Aborted(AbortReason::HopLimit))
        );
        assert!(!runner.is_running());
    }

    #[test]
    fn blocks_on_unsignaled_completion_and_resumes_after_signal() {
        let seen = seen();
        let completion = Completion::new();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(WaitOnce(completion.clone()))
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert!(seen.borrow().is_empty());

        completion.signal();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["after"]);
    }

    #[test]
    fn already_complete_wait_does_not_block() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(WaitOnce(Completion::done())));
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
    }

    #[test]
    fn multi_phase_step_resumes_across_waits() {
        let seen = seen();
        let resume_count = Rc::new(Cell::new(0));
        let first = Completion::new();
        let second = Completion::new();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(TwoPhase {
                    first: first.clone(),
                    second: second.clone(),
                    resumes: resume_count.clone(),
                })
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();

        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        first.signal();
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert!(seen.borrow().is_empty());
        second.signal();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["after"]);
        assert_eq!(resume_count.get(), 3, "wait, wait, done");
    }

    #[test]
    #[allow(clippy::similar_names)] // 'called' and 'caller' are the point of the test
    fn multi_phase_step_resumes_after_a_subroutine_call() {
        let seen = seen();
        let resume_count = Rc::new(Cell::new(0));
        let mut library = Library::new();
        let called = library.insert(Sequence::new("called").with_step(probe("called", &seen)));
        let caller = library.insert(
            Sequence::new("caller")
                .with_step(CallThenDone {
                    target: called,
                    resumes: resume_count.clone(),
                })
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(caller, None).unwrap();

        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["called", "after"]);
        assert_eq!(resume_count.get(), 2, "call, then done");
    }

    #[test]
    #[cfg(feature = "std")] // panic isolation needs an unwinder
    fn panicking_step_is_skipped_and_the_chain_continues() {
        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(Panics)
                .with_step(probe("after", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["after"]);

        let events: Vec<_> = runner.drain_events().collect();
        assert!(events.iter().any(|e| matches!(
            e,
            RunnerEvent::StepFailed { index: 0, reason, .. } if reason == "boom"
        )));
    }

    #[test]
    fn missing_step_is_skipped_with_a_warning() {
        struct Holey {
            library: Library,
            hole: (SequenceRef, usize),
        }
        impl SequenceFacts for Holey {
            fn step_count(&mut self, s: SequenceRef) -> Option<usize> {
                self.library.step_count(s)
            }
            fn step_facts(&mut self, s: SequenceRef, i: usize) -> Option<StepFacts> {
                ((s, i) != self.hole)
                    .then(|| self.library.step_facts(s, i))
                    .flatten()
            }
        }
        impl SequenceSource for Holey {
            fn start_step(
                &mut self,
                s: SequenceRef,
                i: usize,
                ctx: &mut Context<'_>,
            ) -> Option<Progress> {
                ((s, i) != self.hole)
                    .then(|| self.library.start_step(s, i, ctx))
                    .flatten()
            }
        }

        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(probe("before", &seen))
                .with_step(steps::Stop) // stands where the hole is
                .with_step(probe("after", &seen)),
        );
        let mut holey = Holey {
            library,
            hole: (a, 1),
        };
        let mut runner = Runner::default();
        let mut services = TypeMap::new();
        runner.start(a, None).unwrap();
        assert_eq!(
            runner.advance(&mut holey, &mut services),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(
            *seen.borrow(),
            vec!["before", "after"],
            "the hole is skipped, not fatal — and the Stop behind it never ran"
        );
    }

    #[test]
    fn guard_dropped_when_finished_is_reported() {
        let released = Rc::new(Cell::new(false));
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a"));
        let mut runner = Runner::default();
        let flag = released.clone();
        runner.set_guard_factory(move || Box::new(DropFlag(flag.clone())));

        runner.start(a, None).unwrap();
        assert!(!released.get(), "guard held while the chain is in flight");
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert!(
            released.get(),
            "guard released by the time Finished is reported"
        );
    }

    #[test]
    fn guard_dropped_on_external_stop() {
        let released = Rc::new(Cell::new(false));
        let completion = Completion::new();
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(WaitOnce(completion)));
        let mut runner = Runner::default();
        let flag = released.clone();
        runner.set_guard_factory(move || Box::new(DropFlag(flag.clone())));

        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert!(!released.get());
        assert!(runner.stop());
        assert!(released.get(), "external stop releases the guard too");
        assert!(!runner.stop(), "second stop finds nothing to stop");
    }

    #[test]
    fn guard_dropped_when_hop_limit_aborts() {
        let released = Rc::new(Cell::new(false));
        let mut library = Library::new();
        let looped = library.insert(Sequence::new("loop"));
        library.get_mut(looped).unwrap().push(steps::Branch {
            condition: None,
            if_true: Some(looped),
            if_false: None,
        });
        let mut runner = Runner::default();
        let flag = released.clone();
        runner.set_guard_factory(move || Box::new(DropFlag(flag.clone())));

        runner.start(looped, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Aborted(AbortReason::HopLimit))
        );
        assert!(
            released.get(),
            "guard released by the time Aborted is reported"
        );
    }

    #[test]
    fn second_start_is_refused() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(WaitOnce(Completion::new())));
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert_eq!(runner.start(a, None), Err(StartError::AlreadyRunning));
        assert!(
            runner.is_running(),
            "the refused start did not disturb the chain"
        );
    }

    #[test]
    fn events_report_started_steps_and_skip_disabled_ones() {
        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(probe("a", &seen))
                .with_step(DisabledProbe(probe("disabled", &seen)))
                .with_step(probe("b", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["a", "b"]);

        let events: Vec<_> = runner.drain_events().collect();
        let started: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                RunnerEvent::StepStarted { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec![0, 2], "no start event for the disabled step");
        assert!(
            events.iter().any(|e| matches!(
                e,
                RunnerEvent::StepSkipped {
                    index: 1,
                    why: SkipReason::Disabled,
                    ..
                }
            )),
            "the disabled step is reported as skipped, not silently passed over"
        );
    }

    #[test]
    fn instigator_reaches_steps() {
        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(SeesInstigator {
            expected: 7,
            seen: seen.clone(),
        }));
        let mut runner = Runner::default();
        runner.start(a, Some(Box::new(7_u32))).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(*seen.borrow(), vec!["instigator-ok"]);
    }

    #[test]
    fn current_names_the_blocked_step() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Note {
                    message: "x".into(),
                })
                .with_step(WaitOnce(Completion::new())),
        );
        let mut runner = Runner::default();
        assert_eq!(runner.current(), None);
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Poll::Pending);
        assert_eq!(runner.current(), Some((a, 1)));
    }

    #[test]
    fn advance_when_idle_is_idle() {
        let mut library = Library::new();
        let mut runner = Runner::default();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Idle)
        );
    }

    /// A state machine that never finishes.
    struct NeverDone;
    impl StepRun for NeverDone {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Resume(Box::new(NeverDone))
        }
    }

    /// A wait that always resolves immediately.
    struct AlwaysDoneWait;
    impl StepRun for AlwaysDoneWait {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Wait(Completion::done())
        }
    }

    /// Starts a test state machine.
    struct StartsRunaway(fn() -> Box<dyn StepRun>);
    impl Step for StartsRunaway {
        fn summary(&self) -> String {
            "runaway".to_owned()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Resume((self.0)())
        }
    }

    fn run_runaway(make: fn() -> Box<dyn StepRun>) -> Poll<Outcome> {
        let mut library = Library::new();
        let sequence = library.insert(Sequence::new("runaway").with_step(StartsRunaway(make)));
        let mut runner = Runner::default();
        let mut services = TypeMap::new();
        runner.start(sequence, None).unwrap();
        runner.advance(&mut library, &mut services)
    }

    #[test]
    fn a_state_machine_that_never_finishes_aborts_instead_of_spinning() {
        assert_eq!(
            run_runaway(|| Box::new(NeverDone)),
            Poll::Ready(Outcome::Aborted(AbortReason::ResumeLimit))
        );
    }

    #[test]
    fn a_wait_that_never_blocks_aborts_instead_of_spinning() {
        assert_eq!(
            run_runaway(|| Box::new(AlwaysDoneWait)),
            Poll::Ready(Outcome::Aborted(AbortReason::ResumeLimit))
        );
    }

    #[test]
    fn a_legitimate_multi_phase_step_stays_under_the_resume_guard() {
        let mut library = Library::new();
        let sequence = library.insert(Sequence::new("phases").with_step(PhasedStep {
            waits: 3,
            resumes: Rc::new(Cell::new(0)),
        }));
        let mut runner = Runner::default();
        let mut services = TypeMap::new();
        runner.start(sequence, None).unwrap();
        assert_eq!(
            runner.advance(&mut library, &mut services),
            Poll::Ready(Outcome::Finished)
        );
    }

    /// Waits on an already-complete handle a fixed number of times.
    struct PhasedStep {
        waits: usize,
        resumes: Rc<Cell<usize>>,
    }
    struct PhasedRun {
        left: usize,
    }
    impl StepRun for PhasedRun {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            if self.left == 0 {
                Progress::Done
            } else {
                self.left -= 1;
                Progress::Wait(Completion::done())
            }
        }
    }
    impl Step for PhasedStep {
        fn summary(&self) -> String {
            "phased".to_owned()
        }
        fn flow(&self) -> Flow {
            Flow::Continue
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            self.resumes.set(self.resumes.get() + 1);
            Progress::Resume(Box::new(PhasedRun { left: self.waits }))
        }
    }

    #[test]
    fn the_event_buffer_is_bounded_when_nobody_drains() {
        let cap = 8;
        let mut library = Library::new();
        let mut sequence = Sequence::new("many");
        for _ in 0..100 {
            sequence.push(steps::run("tick", |_ctx| {}));
        }
        let sequence = library.insert(sequence);

        let mut runner = Runner::new(RunnerConfig::default().max_buffered_events(cap));
        let mut services = TypeMap::new();
        runner.start(sequence, None).unwrap();
        assert_eq!(
            runner.advance(&mut library, &mut services),
            Poll::Ready(Outcome::Finished)
        );

        let events: Vec<_> = runner.drain_events().collect();
        assert_eq!(events.len(), cap);
        assert_eq!(
            events.last(),
            Some(&RunnerEvent::StepStarted {
                sequence,
                index: 99
            })
        );
    }

    /// Calls a subroutine and counts later resumes.
    struct CallsThenCounts {
        target: SequenceRef,
        resumes_after_call: Rc<Cell<usize>>,
    }
    struct CallsThenCountsRun {
        target: SequenceRef,
        called: bool,
        resumes_after_call: Rc<Cell<usize>>,
    }
    impl StepRun for CallsThenCountsRun {
        fn resume(&mut self, _ctx: &mut Context<'_>) -> Progress {
            if self.called {
                self.resumes_after_call
                    .set(self.resumes_after_call.get() + 1);
                return Progress::Done;
            }
            self.called = true;
            Progress::Call(self.target)
        }
    }
    impl Step for CallsThenCounts {
        fn summary(&self) -> String {
            "calls then counts".to_owned()
        }
        fn start(&self, _ctx: &mut Context<'_>) -> Progress {
            Progress::Resume(Box::new(CallsThenCountsRun {
                target: self.target,
                called: false,
                resumes_after_call: self.resumes_after_call.clone(),
            }))
        }
    }

    #[test]
    fn goto_from_a_subroutine_ends_the_caller() {
        let seen = seen();
        let mut library = Library::new();
        let landing = library.insert(Sequence::new("landing").with_step(probe("landing", &seen)));
        let inner = library.insert(
            Sequence::new("inner")
                .with_step(probe("inner", &seen))
                .with_step(steps::Branch {
                    condition: None,
                    if_true: Some(landing),
                    if_false: None,
                }),
        );
        let outer = library.insert(
            Sequence::new("outer")
                .with_step(steps::Call {
                    sequence: Some(inner),
                })
                .with_step(probe("never", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(outer, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(
            *seen.borrow(),
            vec!["inner", "landing"],
            "the goto unwound the caller's frame too"
        );
    }

    #[test]
    fn goto_none_finishes_the_chain() {
        let seen = seen();
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Stop)
                .with_step(probe("never", &seen)),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn goto_does_not_resume_the_callers_machine() {
        let resumes_after_call = Rc::new(Cell::new(0));
        let mut library = Library::new();
        let inner = library.insert(Sequence::new("inner").with_step(steps::Stop));
        let outer = library.insert(Sequence::new("outer").with_step(CallsThenCounts {
            target: inner,
            resumes_after_call: resumes_after_call.clone(),
        }));
        let mut runner = Runner::default();
        runner.start(outer, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(resumes_after_call.get(), 0);
    }

    #[test]
    fn a_returning_subroutine_still_resumes_the_callers_machine() {
        let resumes_after_call = Rc::new(Cell::new(0));
        let mut library = Library::new();
        let inner = library.insert(Sequence::new("inner"));
        let outer = library.insert(Sequence::new("outer").with_step(CallsThenCounts {
            target: inner,
            resumes_after_call: resumes_after_call.clone(),
        }));
        let mut runner = Runner::default();
        runner.start(outer, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert_eq!(resumes_after_call.get(), 1);
    }

    #[test]
    fn a_note_reaches_the_event_stream() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::run("speak", |ctx| ctx.note("the elder is asleep")))
                .with_step(steps::Note {
                    message: "and the gate is shut".to_owned(),
                }),
        );
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );

        let notes: Vec<_> = runner
            .drain_events()
            .filter_map(|e| match e {
                RunnerEvent::Note {
                    message,
                    sequence,
                    index,
                } => Some((sequence, index, message)),
                _ => None,
            })
            .collect();
        assert_eq!(
            notes,
            vec![
                (a, 0, "the elder is asleep".to_owned()),
                (a, 1, "and the gate is shut".to_owned()),
            ],
            "each note is tagged with where it was said"
        );
    }

    #[test]
    fn advancing_with_no_chain_is_ready_and_idle() {
        let mut library = Library::new();
        let mut runner = Runner::default();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Idle)
        );
    }

    #[test]
    fn a_missing_sequence_is_reported_not_logged() {
        let mut library = Library::new();
        let mut runner = Runner::default();
        let stranger = SequenceRef::from_raw(99);
        runner.start(stranger, None).unwrap();
        assert_eq!(
            advance(&mut runner, &mut library),
            Poll::Ready(Outcome::Finished)
        );
        assert!(runner.drain_events().any(|e| matches!(
            e,
            RunnerEvent::SequenceMissing { sequence } if sequence == stranger
        )));
    }

    #[test]
    fn config_setters_chain() {
        let config = RunnerConfig::default()
            .max_call_depth(8)
            .max_hops_per_advance(16)
            .max_resumes_per_advance(4)
            .max_buffered_events(2);
        assert_eq!(config.max_call_depth, 8);
        assert_eq!(config.max_hops_per_advance, 16);
        assert_eq!(config.max_resumes_per_advance, 4);
        assert_eq!(config.max_buffered_events, 2);
    }
}
