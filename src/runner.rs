//! The machine that walks sequences: a flat trampoline with guards.
//!
//! The runner is **time-free**. It has no clock, no frames, no dt — the host calls
//! [`advance`](Runner::advance) whenever something may have changed (a game loop once per
//! frame, a test right after signaling a [`Completion`](crate::Completion)), and between
//! calls the runner is inert data. It does not know whether its host is realtime.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::context::{ChainState, Context, TypeMap};
use crate::source::{SequenceRef, SequenceSource};
use crate::step::{Progress, StepRun};

/// Tunables for the runner's guards. The defaults are the C# original's constants.
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// Maximum simultaneously active sequences in one chain — the base sequence plus
    /// nested [`Call`](Progress::Call)s. Exceeding it is a subroutine chain that includes
    /// itself: the whole chain stops, as a normal finish (an authoring error is not an
    /// engine failure).
    pub max_call_depth: u32,
    /// Maximum trampoline hops (branch transitions between sequences) within a single
    /// [`advance`](Runner::advance). Exceeding it is an authored loop with no blocking
    /// step: the chain aborts.
    pub max_hops_per_advance: u32,
    /// Log every step start. The C# `traceSteps`: sequences are data, so the log is the
    /// debugger.
    pub trace: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_call_depth: 32,
            max_hops_per_advance: 64,
            trace: false,
        }
    }
}

/// Where a chain stands after an [`advance`](Runner::advance).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No chain is running.
    Idle,
    /// The chain is holding on an unsignaled [`Completion`](crate::Completion); advance
    /// again once it may have been signaled.
    Blocked,
    /// The chain ran out of steps and is over. The [`ChainGuard`] was dropped before
    /// this was returned.
    Finished,
    /// A guard killed the chain. The [`ChainGuard`] was dropped before this was
    /// returned.
    Aborted(AbortReason),
}

/// Why a chain was aborted. External [`stop`](Runner::stop) is reported by that call
/// itself, not through here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortReason {
    /// The per-advance trampoline hop guard fired: an authored loop with no blocking
    /// step.
    HopLimit,
}

/// Something that happened during an [`advance`](Runner::advance), for hosts that
/// surface progress — the editor's executing highlight listens to `StepStarted`.
/// Drain with [`Runner::drain_events`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerEvent {
    /// An enabled step began executing.
    StepStarted {
        /// The sequence the step belongs to.
        sequence: SequenceRef,
        /// The step's index within that sequence.
        index: usize,
    },
    /// A step panicked; it was logged and skipped, and the chain continued.
    StepFailed {
        /// The sequence the step belongs to.
        sequence: SequenceRef,
        /// The step's index within that sequence.
        index: usize,
        /// The panic payload, when it was a string.
        reason: String,
    },
}

/// Why [`Runner::start`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// A chain is already in flight. Sequences are atomic, and the second request is a
    /// content bug, not a queue — the refusal is logged loudly.
    AlreadyRunning,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("a sequence chain is already running"),
        }
    }
}

impl std::error::Error for StartError {}

/// An opaque hold the host acquires for the life of a chain — the save-gate seam.
///
/// The embedder installs a factory with [`Runner::set_guard_factory`]; the runner
/// acquires one guard per chain and **drops it on every completion path** — finish,
/// abort, external stop — *before* the completion is reported, so a listener that saves
/// the moment a conversation ends finds the gate already open. `Drop` is the release,
/// which is what makes the hold unstrandable.
pub type ChainGuard = Box<dyn Any>;

/// One active sequence within a chain: which sequence, which step, and the step's
/// in-flight state machine when it has one.
struct Frame {
    sequence: SequenceRef,
    index: usize,
    run: Option<Box<dyn StepRun>>,
}

/// Everything one chain owns while in flight.
struct Chain {
    state: ChainState,
    /// Bottom frame is the trampoline's current sequence; frames above it are nested
    /// [`Progress::Call`] subroutines. All frames share the one [`ChainState`], which is
    /// why a nested stop or branch ends the caller too.
    frames: Vec<Frame>,
    pending: Option<crate::Completion>,
    guard: Option<ChainGuard>,
}

/// Internal outcome of driving a chain as far as it will go.
enum Drive {
    Blocked,
    Finished,
    Aborted(AbortReason),
}

/// Walks sequences from a [`SequenceSource`], one chain at a time.
///
/// A chain is one press of the "go" button: a sequence, every sequence it branches to,
/// and every subroutine along the way. The runner owns all run state — the sequences
/// themselves stay immutable shared data.
///
/// # Examples
///
/// ```
/// use plotline::{Library, Runner, Sequence, Status, TypeMap, steps};
///
/// let mut library = Library::new();
/// let hello = library.insert(
///     Sequence::new("hello").with_step(steps::Log { message: "Hi.".into() }),
/// );
///
/// let mut runner = Runner::default();
/// let mut services = TypeMap::new();
/// runner.start(hello, None).unwrap();
/// assert_eq!(runner.advance(&mut library, &mut services), Status::Finished);
/// ```
pub struct Runner {
    config: RunnerConfig,
    guard_factory: Option<Box<dyn FnMut() -> ChainGuard>>,
    chain: Option<Chain>,
    events: Vec<RunnerEvent>,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new(RunnerConfig::default())
    }
}

impl Runner {
    /// A runner with the given guard tunables and no chain in flight.
    #[must_use]
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            config,
            guard_factory: None,
            chain: None,
            events: Vec::new(),
        }
    }

    /// Installs the [`ChainGuard`] factory — called once per chain start; the returned
    /// guard is dropped when the chain ends, however it ends.
    pub fn set_guard_factory(&mut self, factory: impl FnMut() -> ChainGuard + 'static) {
        self.guard_factory = Some(Box::new(factory));
    }

    /// The guard tunables, adjustable between (or, harmlessly, during) chains — hosts
    /// expose them as settings and write them through here.
    pub fn config_mut(&mut self) -> &mut RunnerConfig {
        &mut self.config
    }

    /// Begins a chain at `sequence`. `instigator` is whatever started it — a trigger, an
    /// NPC, `None` for scripted starts — and is visible to steps through
    /// [`Context::instigator`](crate::Context::instigator).
    ///
    /// Nothing executes until the first [`advance`](Runner::advance).
    ///
    /// # Errors
    ///
    /// [`StartError::AlreadyRunning`] when a chain is in flight; the refusal is also
    /// logged, because the second request is a content bug someone should see.
    pub fn start(
        &mut self,
        sequence: SequenceRef,
        instigator: Option<Box<dyn Any>>,
    ) -> Result<(), StartError> {
        if self.chain.is_some() {
            log::warn!(
                "A sequence chain is already running; refusing to start another. \
                 Sequences are atomic, and the second request is a content bug, not a queue."
            );
            return Err(StartError::AlreadyRunning);
        }
        self.chain = Some(Chain {
            state: ChainState {
                instigator,
                ..ChainState::default()
            },
            frames: vec![Frame {
                sequence,
                index: 0,
                run: None,
            }],
            pending: None,
            guard: self.guard_factory.as_mut().map(|factory| factory()),
        });
        Ok(())
    }

    /// Drives the chain until it blocks on an unsignaled completion, finishes, or a
    /// guard fires. Call it whenever completions may have been signaled; between calls
    /// the runner is inert.
    ///
    /// On [`Status::Finished`] and [`Status::Aborted`] the [`ChainGuard`] has already
    /// been dropped when this returns.
    pub fn advance(&mut self, source: &mut dyn SequenceSource, services: &mut TypeMap) -> Status {
        let Some(mut chain) = self.chain.take() else {
            return Status::Idle;
        };
        match Self::drive(&mut chain, &self.config, &mut self.events, source, services) {
            Drive::Blocked => {
                self.chain = Some(chain);
                Status::Blocked
            }
            Drive::Finished => {
                drop(chain.guard.take()); // the gate opens before anyone hears "finished"
                Status::Finished
            }
            Drive::Aborted(reason) => {
                drop(chain.guard.take());
                Status::Aborted(reason)
            }
        }
    }

    /// Externally ends the chain, abandoning the in-flight step: its state machine is
    /// dropped, and any completion it was waiting on becomes an inert orphan. Returns
    /// whether a chain was actually running. The [`ChainGuard`] is dropped before this
    /// returns — the host surfaces "aborted" however it likes.
    pub fn stop(&mut self) -> bool {
        match self.chain.take() {
            Some(mut chain) => {
                drop(chain.guard.take());
                true
            }
            None => false,
        }
    }

    /// Whether a chain is in flight (running or blocked).
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.chain.is_some()
    }

    /// The sequence and step index currently executing or blocked — the innermost
    /// subroutine when calls are nested. `None` when idle.
    #[must_use]
    pub fn current(&self) -> Option<(SequenceRef, usize)> {
        let frame = self.chain.as_ref()?.frames.last()?;
        Some((frame.sequence, frame.index))
    }

    /// Drains the events accumulated by [`advance`](Runner::advance) calls, oldest
    /// first.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, RunnerEvent> {
        self.events.drain(..)
    }

    // One function on purpose: this loop *is* the trampoline, and splitting it would
    // scatter the control flow it exists to make readable.
    #[allow(clippy::too_many_lines)]
    fn drive(
        chain: &mut Chain,
        config: &RunnerConfig,
        events: &mut Vec<RunnerEvent>,
        source: &mut dyn SequenceSource,
        services: &mut TypeMap,
    ) -> Drive {
        let mut hops: u32 = 0;
        // "The current step should be resumed or completed now" — set when a wait
        // resolves or a subroutine frame pops.
        let mut resume_current = false;

        loop {
            // A pending wait gates everything.
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
                    let caught = {
                        let Chain { state, frames, .. } = &mut *chain;
                        let machine = frames
                            .last_mut()
                            .expect("a chain always has a frame")
                            .run
                            .as_mut()
                            .expect("checked above");
                        let mut ctx = Context::new(services, state);
                        catch_unwind(AssertUnwindSafe(|| machine.resume(&mut ctx)))
                    };
                    match caught {
                        Ok(progress) => {
                            Self::apply_progress(chain, config, progress, &mut resume_current);
                        }
                        Err(payload) => {
                            // `&*payload`, not `&payload`: a `&Box<dyn Any>` would unsize-coerce the
                            // Box itself into the trait object and every downcast would miss.
                            Self::fail_step(chain, events, source, sequence, index, &*payload);
                        }
                    }
                    continue;
                }
                // A plain step's wait or call resolved: the step is done.
                Self::top(chain).index += 1;
            }

            // About to start the next step: a stop (or exhaustion) ends this sequence.
            let (sequence, index) = {
                let frame = Self::top(chain);
                (frame.sequence, frame.index)
            };
            let ended = chain.state.stopped
                || match source.step_count(sequence) {
                    Some(count) => index >= count,
                    None => {
                        log::warn!(
                            "Sequence {} did not resolve; ending it.",
                            source.name(sequence)
                        );
                        true
                    }
                };
            if ended {
                if chain.frames.len() > 1 {
                    // A subroutine ended: its Call step in the frame below is resolved.
                    chain.frames.pop();
                    resume_current = true;
                    continue;
                }
                match chain.state.take_next() {
                    Some(next) => {
                        hops += 1;
                        if hops > config.max_hops_per_advance {
                            log::error!(
                                "Sequence chain hopped {} times in one advance at '{}' — \
                                 an authored loop with no blocking step. Aborting.",
                                hops,
                                source.name(next)
                            );
                            return Drive::Aborted(AbortReason::HopLimit);
                        }
                        chain.frames[0] = Frame {
                            sequence: next,
                            index: 0,
                            run: None,
                        };
                    }
                    None => return Drive::Finished,
                }
                continue;
            }

            // Start the step.
            match source.step_facts(sequence, index) {
                None => {
                    // A null slot left by a renamed class: tolerated, loudly.
                    log::warn!(
                        "'{}' step {} is empty; skipping.",
                        source.name(sequence),
                        index
                    );
                    Self::top(chain).index += 1;
                }
                Some(facts) if !facts.enabled => {
                    if config.trace {
                        log::debug!(
                            "'{}' step {}: (disabled) {}",
                            source.name(sequence),
                            index,
                            facts.summary
                        );
                    }
                    Self::top(chain).index += 1;
                }
                Some(facts) => {
                    if config.trace {
                        log::debug!(
                            "'{}' step {}: {}",
                            source.name(sequence),
                            index,
                            facts.summary
                        );
                    }
                    events.push(RunnerEvent::StepStarted { sequence, index });
                    let caught = {
                        let Chain { state, .. } = &mut *chain;
                        let mut ctx = Context::new(services, state);
                        catch_unwind(AssertUnwindSafe(|| {
                            source.start_step(sequence, index, &mut ctx)
                        }))
                    };
                    match caught {
                        Ok(Some(progress)) => {
                            Self::apply_progress(chain, config, progress, &mut resume_current);
                        }
                        Ok(None) => {
                            log::warn!(
                                "'{}' step {} vanished between describing and running; skipping.",
                                source.name(sequence),
                                index
                            );
                            Self::top(chain).index += 1;
                        }
                        Err(payload) => {
                            // `&*payload`, not `&payload`: a `&Box<dyn Any>` would unsize-coerce the
                            // Box itself into the trait object and every downcast would miss.
                            Self::fail_step(chain, events, source, sequence, index, &*payload);
                        }
                    }
                }
            }
        }
    }

    fn top(chain: &mut Chain) -> &mut Frame {
        chain.frames.last_mut().expect("a chain always has a frame")
    }

    fn apply_progress(
        chain: &mut Chain,
        config: &RunnerConfig,
        progress: Progress,
        resume_current: &mut bool,
    ) {
        match progress {
            Progress::Done => {
                let frame = Self::top(chain);
                frame.run = None;
                frame.index += 1;
            }
            Progress::Wait(completion) => {
                // The frame's machine, if any, stays put: it is resumed when this
                // resolves. An already-signaled handle is caught by the loop's pending
                // check without blocking.
                chain.pending = Some(completion);
            }
            Progress::Call(target) => {
                if chain.frames.len() >= config.max_call_depth as usize {
                    log::error!(
                        "Sequence chain exceeds {} levels of nesting — a subroutine chain \
                         that includes itself. Stopping.",
                        config.max_call_depth
                    );
                    // The whole chain stops (a normal finish, not an abort), and the
                    // refused call resolves immediately.
                    chain.state.stopped = true;
                    if Self::top(chain).run.is_some() {
                        *resume_current = true;
                    } else {
                        Self::top(chain).index += 1;
                    }
                } else {
                    chain.frames.push(Frame {
                        sequence: target,
                        index: 0,
                        run: None,
                    });
                }
            }
            Progress::Resume(machine) => {
                Self::top(chain).run = Some(machine);
                *resume_current = true;
            }
        }
    }

    fn fail_step(
        chain: &mut Chain,
        events: &mut Vec<RunnerEvent>,
        source: &mut dyn SequenceSource,
        sequence: SequenceRef,
        index: usize,
        payload: &(dyn Any + Send),
    ) {
        let reason = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        log::error!(
            "'{}' step {} panicked ({reason}); skipping it. The chain continues, but \
             whatever that step was mid-way through may be half-done.",
            source.name(sequence),
            index
        );
        events.push(RunnerEvent::StepFailed {
            sequence,
            index,
            reason,
        });
        let frame = Self::top(chain);
        frame.run = None;
        frame.index += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::completion::Completion;
    use crate::sequence::{Library, Sequence};
    use crate::source::SequenceFacts;
    use crate::step::{Flow, Step, StepFacts};
    use crate::steps;

    /// Records that it ran, so tests can observe execution order after the chain's own
    /// state is gone.
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

    /// A probe that reports itself disabled.
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

    /// Waits once on an externally held completion.
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
    struct Panics;

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

    /// A two-phase step: waits on `first`, then on `second`, counting its resumptions.
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

    /// A multi-phase step that calls a sequence, then finishes when the call returns.
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

    /// Asserts the instigator is a specific u32 and records the answer.
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

    /// Sets a shared flag when dropped — the observable stand-in for a save gate.
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

    fn advance(runner: &mut Runner, library: &mut Library) -> Status {
        let mut services = TypeMap::new();
        runner.advance(library, &mut services)
    }

    #[test]
    fn finishes_an_empty_sequence() {
        let mut library = Library::new();
        let empty = library.insert(Sequence::new("empty"));
        let mut runner = Runner::default();
        runner.start(empty, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn trampoline_runs_flat_across_branches() {
        // A → B → C by unconditional branches, all in one advance, no stack growth
        // observable from outside: just Finished with every probe seen in order.
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn take_next_rearms_stopped_for_the_successor() {
        // If `stopped` leaked into the successor, its steps would never run.
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn subroutine_shares_context_so_a_nested_stop_ends_the_caller() {
        // The C# rule: subroutines run against the same context, so Stop propagates.
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(
            *seen.borrow(),
            vec!["before"],
            "the nested Stop ended the caller"
        );
    }

    #[test]
    fn subroutine_flags_survive_into_the_caller() {
        // Same context also means the blackboard is shared: a flag set in a subroutine
        // steers a branch in the caller.
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(
            *seen.borrow(),
            vec!["c"],
            "the caller's remaining steps are severed by the nested branch"
        );
    }

    #[test]
    fn call_depth_overflow_finishes_not_aborts() {
        // A subroutine chain that includes itself: an authoring error, stopped as a
        // normal finish — not an engine failure, so not an abort.
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a"));
        library
            .get_mut(a)
            .unwrap()
            .push(steps::Call { sequence: Some(a) });
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
    }

    #[test]
    fn hop_limit_aborts_an_authored_loop() {
        // A sequence that branches to itself with no blocking step: aborted.
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
            Status::Aborted(AbortReason::HopLimit)
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
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
        assert!(seen.borrow().is_empty());

        completion.signal();
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["after"]);
    }

    #[test]
    fn already_complete_wait_does_not_block() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(WaitOnce(Completion::done())));
        let mut runner = Runner::default();
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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

        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
        first.signal();
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
        assert!(seen.borrow().is_empty());
        second.signal();
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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

        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["called", "after"]);
        assert_eq!(resume_count.get(), 2, "call, then done");
    }

    #[test]
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["after"]);

        let events: Vec<_> = runner.drain_events().collect();
        assert!(events.iter().any(|e| matches!(
            e,
            RunnerEvent::StepFailed { index: 0, reason, .. } if reason == "boom"
        )));
    }

    #[test]
    fn missing_step_is_skipped_with_a_warning() {
        // A source with a hole where a step should be — the serialized-storage case.
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
        assert_eq!(runner.advance(&mut holey, &mut services), Status::Finished);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
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
            Status::Aborted(AbortReason::HopLimit)
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
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["a", "b"]);

        let started: Vec<_> = runner
            .drain_events()
            .filter_map(|e| match e {
                RunnerEvent::StepStarted { index, .. } => Some(index),
                RunnerEvent::StepFailed { .. } => None,
            })
            .collect();
        assert_eq!(started, vec![0, 2], "no event for the disabled step");
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
        assert_eq!(advance(&mut runner, &mut library), Status::Finished);
        assert_eq!(*seen.borrow(), vec!["instigator-ok"]);
    }

    #[test]
    fn current_names_the_blocked_step() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Log {
                    message: "x".into(),
                })
                .with_step(WaitOnce(Completion::new())),
        );
        let mut runner = Runner::default();
        assert_eq!(runner.current(), None);
        runner.start(a, None).unwrap();
        assert_eq!(advance(&mut runner, &mut library), Status::Blocked);
        assert_eq!(runner.current(), Some((a, 1)));
    }

    #[test]
    fn advance_when_idle_is_idle() {
        let mut library = Library::new();
        let mut runner = Runner::default();
        assert_eq!(advance(&mut runner, &mut library), Status::Idle);
    }
}
