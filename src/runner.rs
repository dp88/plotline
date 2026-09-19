//! The runner: a cursor over the steps of a library.

use alloc::vec::Vec;
use core::fmt::{Display, Formatter, Result as FmtResult};
use core::mem;

use crate::rules::Has;
use crate::sequence::{Library, SequenceRef, Step};

/// Limits that stop runaway content.
///
/// Content can loop forever without any action for the host, for example
/// two sequences that jump to each other. The runner aborts such a chain
/// instead of hanging.
///
/// ```
/// use plotline::{Limits, Runner};
///
/// let mut limits = Limits::default();
/// limits.call_depth = 8;
/// let runner = Runner::new(limits);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Limits {
    /// The most sequences a chain may stack through calls. The first
    /// sequence counts as one. The default is 32.
    pub call_depth: usize,
    /// The most steps one [`Runner::advance`] or [`Runner::resume`] may run
    /// before the next action. The default is 10 000.
    pub steps_per_advance: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            call_depth: 32,
            steps_per_advance: 10_000,
        }
    }
}

/// What the runner reports after it moves.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use = "an unread status loses the action the host must perform"]
pub enum Status<'a, A> {
    /// The host performs this action, then calls [`Runner::resume`].
    Act(&'a A),
    /// The runner waits for the answer to an action it already handed out.
    Waiting,
    /// The chain ended.
    Finished,
    /// A limit or a missing sequence stopped the chain.
    Aborted(Abort),
    /// No chain is running.
    Idle,
}

/// Why the runner stopped a chain.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Abort {
    /// A call went deeper than [`Limits::call_depth`].
    CallDepth,
    /// One advance ran more steps than [`Limits::steps_per_advance`].
    StepLimit,
    /// The chain reached a sequence that the library does not hold.
    MissingSequence(SequenceRef),
}

impl Display for Abort {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::CallDepth => f.write_str("the call depth limit was reached"),
            Self::StepLimit => f.write_str("the step limit was reached before an action"),
            Self::MissingSequence(name) => write!(f, "the library has no sequence '{name}'"),
        }
    }
}

/// The host's answer to an action.
///
/// Most actions finish with [`Answer::Done`]. An action can also steer the
/// chain, which is how a dialog choice jumps to the branch the player picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The action finished. The chain continues with the next step.
    Done,
    /// Runs a sequence, then continues after the action.
    Call(SequenceRef),
    /// Clears the call stack and starts a sequence. `None` ends the chain.
    Goto(Option<SequenceRef>),
    /// Leaves the current sequence, as [`Step::Return`] does.
    Return,
}

impl Answer {
    /// Creates an answer that runs `sequence`, then continues.
    #[must_use]
    pub fn call(sequence: impl Into<SequenceRef>) -> Self {
        Self::Call(sequence.into())
    }

    /// Creates an answer that clears the call stack and starts `sequence`.
    #[must_use]
    pub fn goto(sequence: impl Into<SequenceRef>) -> Self {
        Self::Goto(Some(sequence.into()))
    }

    /// Creates an answer that ends the chain.
    #[must_use]
    pub const fn stop() -> Self {
        Self::Goto(None)
    }

    fn to_move(&self) -> Move<'_> {
        match self {
            Self::Done => Move::Next,
            Self::Call(target) => Move::Call(target),
            Self::Goto(target) => Move::Goto(target.as_ref()),
            Self::Return => Move::Return,
        }
    }
}

/// [`Runner::start`] refused, because a chain is already running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

impl Display for Busy {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("a chain is already running")
    }
}

impl core::error::Error for Busy {}

/// Runs one chain of sequences at a time.
///
/// The runner is a cursor. [`Runner::advance`] runs control flow until it
/// reaches an action, then hands that action to the host and waits. The
/// host performs the action in its own code and calls [`Runner::resume`]
/// with its [`Answer`]. The wait is the gap between those two calls, so the
/// runner needs no clock, callback, or async runtime.
///
/// The runner holds only its limits and a stack of positions, so it clones,
/// compares, and, with the `serde` feature, saves in the middle of a chain.
/// A saved runner resumes against the same library it ran on.
///
/// ```
/// use std::collections::BTreeSet;
/// use plotline::{Answer, Library, Requirement, Runner, Status, Step};
///
/// enum Action {
///     Say(&'static str),
/// }
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Key {
///     HasRing,
/// }
///
/// let mut library = Library::new();
/// library.insert(
///     "hub",
///     [
///         Step::act(Action::Say("Have you found my ring?")),
///         Step::branch(Requirement::has(Key::HasRing), "thanks", "later"),
///     ],
/// );
/// library.insert("thanks", [Step::act(Action::Say("You have my thanks."))]);
/// library.insert("later", [Step::act(Action::Say("Come back when you have."))]);
///
/// let held = BTreeSet::from([Key::HasRing]);
/// let mut runner = Runner::default();
/// runner.start("hub").unwrap();
///
/// let mut said = Vec::new();
/// let mut status = runner.advance(&library, &held);
/// while let Status::Act(Action::Say(line)) = status {
///     said.push(*line);
///     status = runner.resume(Answer::Done, &library, &held);
/// }
///
/// assert!(matches!(status, Status::Finished));
/// assert_eq!(said, ["Have you found my ring?", "You have my thanks."]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Runner {
    limits: Limits,
    chain: Option<Chain>,
}

impl Runner {
    /// Creates an idle runner with these limits.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            chain: None,
        }
    }

    /// Starts a chain at the first step of `sequence`.
    ///
    /// Nothing runs until [`Runner::advance`]. A missing sequence aborts the
    /// chain there.
    ///
    /// # Errors
    ///
    /// Returns [`Busy`] when a chain is already running.
    pub fn start(&mut self, sequence: impl Into<SequenceRef>) -> Result<(), Busy> {
        if self.chain.is_some() {
            return Err(Busy);
        }
        self.chain = Some(Chain {
            current: Frame::first_step_of(sequence.into()),
            callers: Vec::new(),
            waiting: false,
        });
        Ok(())
    }

    /// Runs the chain until it reaches an action or ends.
    ///
    /// While an action waits for its answer, this returns
    /// [`Status::Waiting`] and changes nothing, so a host can call it once
    /// per frame. `holder` answers the rules of `When` and `Branch` steps.
    pub fn advance<'l, A, K>(
        &mut self,
        library: &'l Library<A, K>,
        holder: &(impl Has<K> + ?Sized),
    ) -> Status<'l, A> {
        match &self.chain {
            None => Status::Idle,
            Some(chain) if chain.waiting => Status::Waiting,
            Some(_) => self.run(library, holder),
        }
    }

    /// Answers the waiting action, then runs the chain as
    /// [`Runner::advance`] does.
    ///
    /// Call it only after [`Status::Act`]. Without a waiting action, a debug
    /// build panics, and a release build ignores the answer and advances.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "an answer is a command the host hands over, like a message"
    )]
    pub fn resume<'l, A, K>(
        &mut self,
        answer: Answer,
        library: &'l Library<A, K>,
        holder: &(impl Has<K> + ?Sized),
    ) -> Status<'l, A> {
        let Some(chain) = self.chain.as_mut() else {
            return Status::Idle;
        };
        debug_assert!(chain.waiting, "resume was called with no waiting action");
        if chain.waiting {
            chain.waiting = false;
            if let Some(end) = chain.apply(answer.to_move(), &self.limits) {
                self.chain = None;
                return end.into_status();
            }
        }
        self.run(library, holder)
    }

    /// Ends the chain. Returns whether one was running.
    pub fn stop(&mut self) -> bool {
        self.chain.take().is_some()
    }

    /// Returns whether a chain is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.chain.is_some()
    }

    /// Returns the sequence and step index the cursor is at.
    #[must_use]
    pub fn current(&self) -> Option<(&SequenceRef, usize)> {
        let frame = &self.chain.as_ref()?.current;
        Some((&frame.sequence, frame.index))
    }

    /// Returns the action that waits for an answer.
    ///
    /// A host that loads a saved runner calls this to show the pending action
    /// again. It returns `None` when no action waits, or when `library` no
    /// longer holds an action at the cursor.
    #[must_use]
    pub fn waiting_on<'l, A, K>(&self, library: &'l Library<A, K>) -> Option<&'l A> {
        let chain = self.chain.as_ref().filter(|chain| chain.waiting)?;
        let steps = library.get(chain.current.sequence.as_str())?;
        innermost_action(steps.get(chain.current.index)?)
    }

    fn run<'l, A, K>(
        &mut self,
        library: &'l Library<A, K>,
        holder: &(impl Has<K> + ?Sized),
    ) -> Status<'l, A> {
        let Some(chain) = self.chain.as_mut() else {
            return Status::Idle;
        };
        let status = chain.run(library, holder, &self.limits);
        if !matches!(status, Status::Act(_)) {
            self.chain = None;
        }
        status
    }
}

/// One running chain: a call stack of positions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Chain {
    current: Frame,
    callers: Vec<Frame>,
    waiting: bool,
}

/// A position in one sequence. A caller's index stays on its calling step
/// until the callee returns.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Frame {
    sequence: SequenceRef,
    index: usize,
}

impl Frame {
    fn first_step_of(sequence: SequenceRef) -> Self {
        Self { sequence, index: 0 }
    }
}

/// A change to the cursor, decided by a step or by the host's answer.
#[derive(Clone, Copy)]
enum Move<'a> {
    Next,
    Call(&'a SequenceRef),
    Goto(Option<&'a SequenceRef>),
    Return,
}

/// What one step asks for, once its rules are read.
enum Decision<'l, A> {
    Act(&'l A),
    Move(Move<'l>),
}

/// Why a chain ended.
enum End {
    Finished,
    Aborted(Abort),
}

impl End {
    fn into_status<'l, A>(self) -> Status<'l, A> {
        match self {
            Self::Finished => Status::Finished,
            Self::Aborted(abort) => Status::Aborted(abort),
        }
    }
}

impl Chain {
    /// Runs steps until one hands out an action or the chain ends. Never
    /// returns `Waiting` or `Idle`.
    fn run<'l, A, K>(
        &mut self,
        library: &'l Library<A, K>,
        holder: &(impl Has<K> + ?Sized),
        limits: &Limits,
    ) -> Status<'l, A> {
        let mut steps = 0;
        loop {
            let Some(sequence) = library.get(self.current.sequence.as_str()) else {
                let missing = self.current.sequence.clone();
                return Status::Aborted(Abort::MissingSequence(missing));
            };
            let next = match sequence.get(self.current.index) {
                // Falling off the end of a sequence returns from it.
                None => Move::Return,
                Some(step) => {
                    steps += 1;
                    if steps > limits.steps_per_advance {
                        return Status::Aborted(Abort::StepLimit);
                    }
                    match decide(step, holder) {
                        Decision::Act(action) => {
                            self.waiting = true;
                            return Status::Act(action);
                        }
                        Decision::Move(next) => next,
                    }
                }
            };
            if let Some(end) = self.apply(next, limits) {
                return end.into_status();
            }
        }
    }

    /// Moves the cursor. Returns why the chain ended, if it did.
    fn apply(&mut self, next: Move<'_>, limits: &Limits) -> Option<End> {
        match next {
            Move::Next => self.current.index += 1,
            Move::Call(target) => {
                let depth = self.callers.len() + 1;
                if depth >= limits.call_depth {
                    return Some(End::Aborted(Abort::CallDepth));
                }
                let entry = Frame::first_step_of(target.clone());
                let caller = mem::replace(&mut self.current, entry);
                self.callers.push(caller);
            }
            Move::Goto(Some(target)) => {
                self.callers.clear();
                self.current = Frame::first_step_of(target.clone());
            }
            Move::Goto(None) => return Some(End::Finished),
            Move::Return => match self.callers.pop() {
                Some(caller) => {
                    self.current = caller;
                    self.current.index += 1;
                }
                None => return Some(End::Finished),
            },
        }
        None
    }
}

fn decide<'l, A, K>(step: &'l Step<A, K>, holder: &(impl Has<K> + ?Sized)) -> Decision<'l, A> {
    match step {
        Step::Act(action) => Decision::Act(action),
        Step::When { rule, step } => {
            if rule.satisfies(holder) {
                decide(step, holder)
            } else {
                Decision::Move(Move::Next)
            }
        }
        Step::Branch {
            rule,
            if_true,
            if_false,
        } => {
            let target = if rule.satisfies(holder) {
                if_true
            } else {
                if_false
            };
            Decision::Move(Move::Goto(target.as_ref()))
        }
        Step::Call(target) => Decision::Move(Move::Call(target)),
        Step::Goto(target) => Decision::Move(Move::Goto(target.as_ref())),
        Step::Return => Decision::Move(Move::Return),
    }
}

fn innermost_action<A, K>(step: &Step<A, K>) -> Option<&A> {
    match step {
        Step::Act(action) => Some(action),
        Step::When { step, .. } => innermost_action(step),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::rules::Requirement;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Ring,
    }

    type Script = Library<&'static str, Key>;

    fn acts(names: &[&'static str]) -> Vec<Step<&'static str, Key>> {
        names.iter().map(|name| Step::act(*name)).collect()
    }

    /// Drives a chain to its end and records every action the host saw.
    fn drive<'l>(
        runner: &mut Runner,
        library: &'l Script,
        held: &BTreeSet<Key>,
        answer: impl Fn(&str) -> Answer,
    ) -> (Vec<&'static str>, Status<'l, &'static str>) {
        let mut seen = Vec::new();
        let mut status = runner.advance(library, held);
        while let Status::Act(action) = status {
            seen.push(*action);
            status = runner.resume(answer(action), library, held);
        }
        (seen, status)
    }

    fn run(
        library: &Script,
        start: &str,
        held: &[Key],
    ) -> (Vec<&'static str>, Status<'static, &'static str>) {
        let mut runner = Runner::default();
        runner.start(start).unwrap();
        let held = held.iter().copied().collect();
        let (seen, status) = drive(&mut runner, library, &held, |_| Answer::Done);
        let status = match status {
            Status::Finished => Status::Finished,
            Status::Aborted(abort) => Status::Aborted(abort),
            other => panic!("a driven chain ended with {other:?}"),
        };
        (seen, status)
    }

    #[test]
    fn a_sequence_hands_out_its_actions_in_order() {
        let mut library = Script::new();
        library.insert("main", acts(&["a", "b", "c"]));
        assert_eq!(
            run(&library, "main", &[]),
            (vec!["a", "b", "c"], Status::Finished)
        );

        library.insert("empty", []);
        assert_eq!(run(&library, "empty", &[]), (vec![], Status::Finished));
    }

    #[test]
    fn a_when_step_runs_only_when_its_rule_holds() {
        let mut library = Script::new();
        library.insert(
            "main",
            [
                Step::act("greet"),
                Step::when(Requirement::has(Key::Ring), Step::act("thank")),
                Step::act("leave"),
            ],
        );
        assert_eq!(run(&library, "main", &[]).0, ["greet", "leave"]);
        assert_eq!(
            run(&library, "main", &[Key::Ring]).0,
            ["greet", "thank", "leave"]
        );
    }

    #[test]
    fn a_branch_starts_the_side_its_rule_picks() {
        let mut library = Script::new();
        library.insert(
            "main",
            [
                Step::branch(Requirement::has(Key::Ring), "yes", "no"),
                Step::act("never"),
            ],
        );
        library.insert("yes", acts(&["yes"]));
        library.insert("no", acts(&["no"]));
        assert_eq!(run(&library, "main", &[Key::Ring]).0, ["yes"]);
        assert_eq!(run(&library, "main", &[]).0, ["no"]);

        library.insert(
            "ending",
            [Step::Branch {
                rule: Requirement::has(Key::Ring),
                if_true: Some("yes".into()),
                if_false: None,
            }],
        );
        assert_eq!(run(&library, "ending", &[]), (vec![], Status::Finished));
    }

    #[test]
    fn a_call_returns_to_the_step_after_it() {
        let mut library = Script::new();
        library.insert(
            "main",
            [
                Step::act("before"),
                Step::call("sub"),
                Step::when(Requirement::has(Key::Ring), Step::call("sub")),
                Step::act("after"),
            ],
        );
        library.insert("sub", acts(&["inside"]));
        assert_eq!(
            run(&library, "main", &[Key::Ring]).0,
            ["before", "inside", "inside", "after"]
        );
    }

    #[test]
    fn return_leaves_a_sequence_early() {
        let mut library = Script::new();
        library.insert(
            "main",
            [Step::call("sub"), Step::act("after"), Step::Return],
        );
        library.insert(
            "sub",
            [Step::act("inside"), Step::Return, Step::act("skipped")],
        );
        library.insert("root", [Step::Return, Step::act("skipped")]);

        assert_eq!(run(&library, "main", &[]).0, ["inside", "after"]);
        assert_eq!(run(&library, "root", &[]), (vec![], Status::Finished));
    }

    #[test]
    fn goto_clears_the_call_stack() {
        let mut library = Script::new();
        library.insert("main", [Step::call("sub"), Step::act("never")]);
        library.insert("sub", [Step::goto("other")]);
        library.insert("other", acts(&["other"]));
        library.insert(
            "stop",
            [Step::act("last"), Step::stop(), Step::act("never")],
        );

        assert_eq!(
            run(&library, "main", &[]),
            (vec!["other"], Status::Finished)
        );
        assert_eq!(run(&library, "stop", &[]), (vec!["last"], Status::Finished));
    }

    #[test]
    fn an_answer_steers_the_chain() {
        let mut library = Script::new();
        library.insert(
            "main",
            [Step::act("call"), Step::act("goto"), Step::act("never")],
        );
        library.insert("sub", [Step::act("return"), Step::act("never")]);
        library.insert("other", [Step::act("stop"), Step::act("never")]);

        let mut runner = Runner::default();
        runner.start("main").unwrap();
        let (seen, status) = drive(
            &mut runner,
            &library,
            &BTreeSet::new(),
            |action| match action {
                "call" => Answer::call("sub"),
                "return" => Answer::Return,
                "goto" => Answer::goto("other"),
                "stop" => Answer::stop(),
                _ => Answer::Done,
            },
        );
        assert_eq!(seen, ["call", "return", "goto", "stop"]);
        assert_eq!(status, Status::Finished);
    }

    #[test]
    fn the_runner_waits_for_an_answer() {
        let mut library = Script::new();
        library.insert("main", [Step::call("sub"), Step::act("after")]);
        library.insert(
            "sub",
            [Step::when(Requirement::has(Key::Ring), Step::act("ask"))],
        );
        let held = BTreeSet::from([Key::Ring]);

        let mut runner = Runner::default();
        runner.start("main").unwrap();
        assert_eq!(runner.advance(&library, &held), Status::Act(&"ask"));
        assert_eq!(runner.advance(&library, &held), Status::Waiting);
        assert_eq!(runner.waiting_on(&library), Some(&"ask"));
        assert_eq!(runner.current(), Some((&"sub".into(), 0)));

        assert_eq!(
            runner.resume(Answer::Done, &library, &held),
            Status::Act(&"after")
        );
        assert_eq!(runner.current(), Some((&"main".into(), 1)));
    }

    #[test]
    fn one_chain_runs_at_a_time() {
        let mut library = Script::new();
        library.insert("main", acts(&["a"]));
        let held = BTreeSet::new();

        let mut runner = Runner::default();
        assert_eq!(runner.advance(&library, &held), Status::Idle);
        assert!(!runner.stop());

        runner.start("main").unwrap();
        assert_eq!(runner.start("main"), Err(Busy));
        assert!(runner.is_running());
        assert!(runner.stop());
        assert!(!runner.is_running());
        assert_eq!(runner.waiting_on(&library), None);

        runner.start("main").unwrap();
        assert_eq!(runner.advance(&library, &held), Status::Act(&"a"));
        assert_eq!(
            runner.resume(Answer::Done, &library, &held),
            Status::Finished
        );
        assert_eq!(runner.advance(&library, &held), Status::Idle);
        assert_eq!(runner.resume(Answer::Done, &library, &held), Status::Idle);
        assert_eq!(runner.start("main"), Ok(()));
    }

    #[test]
    fn a_missing_sequence_aborts_with_its_name() {
        let mut library = Script::new();
        library.insert("main", [Step::act("a"), Step::call("absent")]);

        let missing = Status::Aborted(Abort::MissingSequence("absent".into()));
        assert_eq!(run(&library, "main", &[]), (vec!["a"], missing.clone()));
        assert_eq!(run(&library, "absent", &[]), (vec![], missing));
    }

    #[test]
    fn a_deep_call_aborts_at_the_call_depth() {
        let mut library = Script::new();
        library.insert("a", [Step::call("b")]);
        library.insert("b", [Step::call("c")]);
        library.insert("c", acts(&["deep"]));
        let held = BTreeSet::new();

        let mut runner = Runner::new(Limits {
            call_depth: 3,
            ..Limits::default()
        });
        runner.start("a").unwrap();
        assert_eq!(runner.advance(&library, &held), Status::Act(&"deep"));

        let mut runner = Runner::new(Limits {
            call_depth: 2,
            ..Limits::default()
        });
        runner.start("a").unwrap();
        assert_eq!(
            runner.advance(&library, &held),
            Status::Aborted(Abort::CallDepth)
        );
        assert!(!runner.is_running());

        library.insert("forever", [Step::call("forever")]);
        assert_eq!(
            run(&library, "forever", &[]).1,
            Status::Aborted(Abort::CallDepth)
        );
    }

    #[test]
    fn a_loop_without_actions_aborts_at_the_step_limit() {
        let mut library = Script::new();
        library.insert("ping", [Step::goto("pong")]);
        library.insert("pong", [Step::goto("ping")]);
        assert_eq!(
            run(&library, "ping", &[]).1,
            Status::Aborted(Abort::StepLimit)
        );
    }

    #[test]
    fn a_wide_call_tree_without_a_loop_is_bounded_too() {
        // Each level calls the next one twice, so the tree holds 2^20 calls.
        let mut library = Script::new();
        for level in 0..20 {
            let next = format!("level-{}", level + 1);
            library.insert(
                format!("level-{level}"),
                [Step::call(next.as_str()), Step::call(next.as_str())],
            );
        }
        library.insert("level-20", []);
        assert_eq!(
            run(&library, "level-0", &[]).1,
            Status::Aborted(Abort::StepLimit)
        );
    }

    #[test]
    fn the_step_limit_counts_steps_between_actions() {
        let mut library = Script::new();
        let mut steps = Vec::new();
        for _ in 0..10 {
            steps.push(Step::when(
                Requirement::has(Key::Ring),
                Step::act("skipped"),
            ));
            steps.push(Step::act("shown"));
        }
        library.insert("main", steps);

        let mut runner = Runner::new(Limits {
            steps_per_advance: 2,
            ..Limits::default()
        });
        runner.start("main").unwrap();
        let (seen, status) = drive(&mut runner, &library, &BTreeSet::new(), |_| Answer::Done);
        assert_eq!(seen.len(), 10);
        assert_eq!(status, Status::Finished);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "no waiting action")]
    fn resume_without_a_waiting_action_panics_in_debug() {
        let mut library = Script::new();
        library.insert("main", acts(&["a"]));
        let mut runner = Runner::default();
        runner.start("main").unwrap();
        let _ = runner.resume(Answer::stop(), &library, &BTreeSet::new());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn resume_without_a_waiting_action_ignores_the_answer_in_release() {
        let mut library = Script::new();
        library.insert("main", acts(&["a"]));
        let mut runner = Runner::default();
        runner.start("main").unwrap();
        assert_eq!(
            runner.resume(Answer::stop(), &library, &BTreeSet::new()),
            Status::Act(&"a")
        );
    }

    #[test]
    fn an_abort_explains_itself() {
        let abort = Abort::MissingSequence("absent".into());
        assert_eq!(abort.to_string(), "the library has no sequence 'absent'");
        assert_eq!(Busy.to_string(), "a chain is already running");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn a_saved_chain_finishes_from_the_copy() {
        let mut library = Script::new();
        library.insert(
            "main",
            [Step::act("one"), Step::call("sub"), Step::act("four")],
        );
        library.insert("sub", acts(&["two", "three"]));
        let held = BTreeSet::new();

        let mut runner = Runner::default();
        runner.start("main").unwrap();
        let _ = runner.advance(&library, &held);
        assert_eq!(
            runner.resume(Answer::Done, &library, &held),
            Status::Act(&"two")
        );

        let saved = serde_json::to_string(&runner).unwrap();
        let mut loaded: Runner = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded, runner);
        assert_eq!(loaded.waiting_on(&library), Some(&"two"));

        let rest = |runner: &mut Runner| {
            let mut seen = Vec::new();
            let mut status = runner.resume(Answer::Done, &library, &held);
            while let Status::Act(action) = status {
                seen.push(*action);
                status = runner.resume(Answer::Done, &library, &held);
            }
            (seen, status)
        };
        assert_eq!(rest(&mut loaded), (vec!["three", "four"], Status::Finished));
        assert_eq!(rest(&mut runner), (vec!["three", "four"], Status::Finished));
    }
}
