//! Sequences of steps, held as data.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt::{Display, Formatter, Result as FmtResult};

use crate::rules::Requirement;

/// The name of a sequence in a [`Library`].
///
/// A step names its target, so a data file can refer to a sequence before
/// it defines it. [`Library::validate`] reports a name that no sequence has.
///
/// ```
/// use plotline::SequenceRef;
///
/// let hub = SequenceRef::from("hub");
/// assert_eq!(hub.as_str(), "hub");
/// assert_eq!(hub.to_string(), "hub");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct SequenceRef(String);

impl SequenceRef {
    /// Returns the name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SequenceRef {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl From<String> for SequenceRef {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl Borrow<str> for SequenceRef {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Display for SequenceRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

/// One step of a sequence.
///
/// `A` is the host's action type: a line of dialog, a camera shake, a grant.
/// The runner never interprets an action. It hands each one to the host and
/// waits for the host's [`Answer`](crate::Answer). `K` is the capability key
/// type that the rules read through [`Has`](crate::Has).
///
/// Every other variant is control flow, and the runner handles it without
/// the host.
///
/// ```
/// use plotline::{Requirement, Step};
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
/// let steps: Vec<Step<Action, Key>> = vec![
///     Step::act(Action::Say("Have you found my ring?")),
///     Step::branch(Requirement::has(Key::HasRing), "thanks", "come-back-later"),
/// ];
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Step<A, K> {
    /// Hands an action to the host and waits for its answer.
    Act(A),
    /// Runs the inner step when the rule holds, and skips it otherwise.
    When {
        /// The rule that decides.
        rule: Requirement<K>,
        /// The step to run.
        step: Box<Step<A, K>>,
    },
    /// Starts one of two sequences, chosen by the rule, as [`Step::Goto`]
    /// does. A `None` target ends the chain.
    Branch {
        /// The rule that decides.
        rule: Requirement<K>,
        /// The target when the rule holds.
        if_true: Option<SequenceRef>,
        /// The target when the rule fails.
        if_false: Option<SequenceRef>,
    },
    /// Runs a sequence, then continues after this step.
    Call(SequenceRef),
    /// Clears the call stack and starts a sequence. `None` ends the chain.
    Goto(Option<SequenceRef>),
    /// Leaves the current sequence and continues after the step that called
    /// it. At the root of the chain, it ends the chain.
    Return,
}

impl<A, K> Step<A, K> {
    /// Creates a step that hands `action` to the host.
    #[must_use]
    pub fn act(action: A) -> Self {
        Self::Act(action)
    }

    /// Creates a step that runs `step` only when `rule` holds.
    #[must_use]
    pub fn when(rule: Requirement<K>, step: Self) -> Self {
        Self::When {
            rule,
            step: Box::new(step),
        }
    }

    /// Creates a step that starts `if_true` when `rule` holds, and `if_false`
    /// otherwise. Use the variant directly for a target that ends the chain.
    #[must_use]
    pub fn branch(
        rule: Requirement<K>,
        if_true: impl Into<SequenceRef>,
        if_false: impl Into<SequenceRef>,
    ) -> Self {
        Self::Branch {
            rule,
            if_true: Some(if_true.into()),
            if_false: Some(if_false.into()),
        }
    }

    /// Creates a step that runs `sequence`, then continues.
    #[must_use]
    pub fn call(sequence: impl Into<SequenceRef>) -> Self {
        Self::Call(sequence.into())
    }

    /// Creates a step that clears the call stack and starts `sequence`.
    #[must_use]
    pub fn goto(sequence: impl Into<SequenceRef>) -> Self {
        Self::Goto(Some(sequence.into()))
    }

    /// Creates a step that ends the chain.
    #[must_use]
    pub const fn stop() -> Self {
        Self::Goto(None)
    }
}

/// Named sequences of steps.
///
/// A library is plain data. With the `serde` feature it loads from a file as
/// a map from each sequence name to its list of steps.
///
/// ```
/// use plotline::{Library, Step};
///
/// let mut library: Library<&str, ()> = Library::new();
/// library.insert("greeting", [Step::act("Hello."), Step::call("farewell")]);
/// library.insert("farewell", [Step::act("Safe roads.")]);
///
/// assert_eq!(library.get("greeting").map(<[_]>::len), Some(2));
/// assert!(library.validate(|_| Vec::new()).is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Library<A, K> {
    sequences: BTreeMap<SequenceRef, Vec<Step<A, K>>>,
}

impl<A, K> Default for Library<A, K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, K> Library<A, K> {
    /// Creates an empty library.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sequences: BTreeMap::new(),
        }
    }

    /// Adds a sequence and returns the steps it replaced.
    pub fn insert(
        &mut self,
        name: impl Into<SequenceRef>,
        steps: impl IntoIterator<Item = Step<A, K>>,
    ) -> Option<Vec<Step<A, K>>> {
        self.sequences
            .insert(name.into(), steps.into_iter().collect())
    }

    /// Returns the steps of a sequence.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[Step<A, K>]> {
        self.sequences.get(name).map(Vec::as_slice)
    }

    /// Returns the steps of a sequence for editing.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Vec<Step<A, K>>> {
        self.sequences.get_mut(name)
    }

    /// Iterates over the sequences in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&SequenceRef, &[Step<A, K>])> {
        self.sequences
            .iter()
            .map(|(name, steps)| (name, steps.as_slice()))
    }

    /// Returns the number of sequences.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    /// Returns whether the library holds no sequences.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }

    /// Reports authoring problems across the library.
    ///
    /// `refs_in` lists the sequences an action names, such as the targets of
    /// a dialog choice. The crate cannot see inside an action, so a host
    /// whose actions name no sequences passes `|_| Vec::new()`.
    ///
    /// Cycles are valid, so they are not reported.
    #[must_use]
    pub fn validate(&self, refs_in: impl Fn(&A) -> Vec<SequenceRef>) -> Vec<Warning> {
        let mut warnings = Vec::new();
        for (name, steps) in &self.sequences {
            let mut report = |index: usize, problem: Problem| {
                warnings.push(Warning {
                    sequence: name.clone(),
                    index,
                    problem,
                });
            };

            for (index, step) in steps.iter().enumerate() {
                self.check_step(step, &refs_in, &mut |problem| report(index, problem));
            }

            let first_exit = steps.iter().position(Self::always_leaves);
            let first_dead = first_exit.map(|index| index + 1);
            if let Some(dead) = first_dead.filter(|dead| *dead < steps.len()) {
                report(dead, Problem::Unreachable);
            }
        }
        warnings
    }

    fn check_step(
        &self,
        step: &Step<A, K>,
        refs_in: &impl Fn(&A) -> Vec<SequenceRef>,
        report: &mut impl FnMut(Problem),
    ) {
        match step {
            Step::Act(action) => {
                for target in refs_in(action) {
                    self.check_target(&target, report);
                }
            }
            Step::When { rule, step } => {
                if let Some(message) = rule.warning() {
                    report(Problem::Rule(message));
                }
                self.check_step(step, refs_in, report);
            }
            Step::Branch {
                rule,
                if_true,
                if_false,
            } => {
                if let Some(message) = rule.warning() {
                    report(Problem::Rule(message));
                }
                for target in if_true.iter().chain(if_false) {
                    self.check_target(target, report);
                }
            }
            Step::Call(target) | Step::Goto(Some(target)) => self.check_target(target, report),
            Step::Goto(None) | Step::Return => {}
        }
    }

    fn check_target(&self, target: &SequenceRef, report: &mut impl FnMut(Problem)) {
        if !self.sequences.contains_key(target) {
            report(Problem::MissingSequence(target.clone()));
        }
    }

    /// Returns whether a step always leaves its sequence, whatever the rules
    /// and the host answer.
    fn always_leaves(step: &Step<A, K>) -> bool {
        matches!(step, Step::Branch { .. } | Step::Goto(_) | Step::Return)
    }
}

/// One problem found by [`Library::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// The sequence that holds the problem.
    pub sequence: SequenceRef,
    /// The position of the step that holds the problem.
    pub index: usize,
    /// What is wrong.
    pub problem: Problem,
}

/// What [`Library::validate`] found wrong with a step.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Problem {
    /// The step names a sequence that the library does not hold.
    MissingSequence(SequenceRef),
    /// A rule in the step reports an authoring warning.
    Rule(String),
    /// The step never runs, because an earlier step always leaves the
    /// sequence.
    Unreachable,
}

impl Display for Warning {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}[{}]: ", self.sequence, self.index)?;
        match &self.problem {
            Problem::MissingSequence(target) => {
                write!(f, "names missing sequence '{target}'")
            }
            Problem::Rule(message) => write!(f, "rule: {message}"),
            Problem::Unreachable => {
                f.write_str("never runs, because an earlier step always leaves")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Action {
        Say(&'static str),
        Choose(Vec<SequenceRef>),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Ring,
    }

    type Script = Library<Action, Key>;

    fn refs_in(action: &Action) -> Vec<SequenceRef> {
        match action {
            Action::Choose(targets) => targets.clone(),
            Action::Say(_) => Vec::new(),
        }
    }

    fn problems(library: &Script) -> Vec<Warning> {
        let mut found = library.validate(refs_in);
        found.sort_by(|a, b| (&a.sequence, a.index).cmp(&(&b.sequence, b.index)));
        found
    }

    fn at(sequence: &str, index: usize, problem: Problem) -> Warning {
        Warning {
            sequence: sequence.into(),
            index,
            problem,
        }
    }

    fn missing(name: &str) -> Problem {
        Problem::MissingSequence(name.into())
    }

    #[test]
    fn a_library_stores_named_sequences() {
        let mut library = Script::new();
        assert!(
            library
                .insert("hub", [Step::act(Action::Say("Hello."))])
                .is_none()
        );
        let replaced = library.insert("hub", [Step::act(Action::Say("Welcome back."))]);

        assert_eq!(replaced, Some(vec![Step::act(Action::Say("Hello."))]));
        assert_eq!(library.len(), 1);
        assert_eq!(
            library.get("hub"),
            Some(&[Step::act(Action::Say("Welcome back."))][..])
        );
        assert!(library.get("absent").is_none());

        library.get_mut("hub").unwrap().push(Step::stop());
        assert_eq!(library.get("hub").map(<[_]>::len), Some(2));
    }

    #[test]
    fn a_sound_library_has_no_warnings() {
        let mut library = Script::new();
        library.insert(
            "hub",
            [
                Step::act(Action::Choose(vec!["yes".into(), "no".into()])),
                Step::when(Requirement::has(Key::Ring), Step::goto("yes")),
                Step::call("no"),
                Step::branch(Requirement::has(Key::Ring), "yes", "no"),
            ],
        );
        library.insert("yes", [Step::Return]);
        library.insert("no", [Step::goto("hub")]);
        assert_eq!(problems(&library), vec![]);
    }

    #[test]
    fn validation_finds_every_missing_target() {
        let mut library = Script::new();
        library.insert(
            "hub",
            [
                Step::act(Action::Choose(vec!["in-an-action".into()])),
                Step::call("called"),
                Step::when(Requirement::has(Key::Ring), Step::goto("nested")),
                Step::branch(Requirement::has(Key::Ring), "if-true", "if-false"),
            ],
        );

        assert_eq!(
            problems(&library),
            vec![
                at("hub", 0, missing("in-an-action")),
                at("hub", 1, missing("called")),
                at("hub", 2, missing("nested")),
                at("hub", 3, missing("if-true")),
                at("hub", 3, missing("if-false")),
            ]
        );
    }

    #[test]
    fn validation_reports_rule_traps() {
        let mut library = Script::new();
        library.insert(
            "hub",
            [
                Step::when(Requirement::any([]), Step::act(Action::Say("Never."))),
                Step::Branch {
                    rule: Requirement::at_least(2, [Requirement::has(Key::Ring)]),
                    if_true: None,
                    if_false: None,
                },
            ],
        );

        let found = problems(&library);
        assert_eq!(found.len(), 2);
        assert!(matches!(&found[0].problem, Problem::Rule(_)));
        assert_eq!(found[0].index, 0);
        assert!(
            matches!(&found[1].problem, Problem::Rule(message) if message.contains("never hold"))
        );
        assert_eq!(found[1].index, 1);
    }

    #[test]
    fn validation_reports_steps_after_an_unconditional_exit() {
        for exit in [
            Step::goto("hub"),
            Step::stop(),
            Step::Return,
            Step::branch(Requirement::has(Key::Ring), "hub", "hub"),
        ] {
            let mut library = Script::new();
            library.insert(
                "hub",
                [
                    exit.clone(),
                    Step::act(Action::Say("Dead.")),
                    Step::act(Action::Say("Also dead.")),
                ],
            );
            assert_eq!(
                problems(&library),
                vec![at("hub", 1, Problem::Unreachable)],
                "{exit:?}"
            );
        }
    }

    #[test]
    fn a_conditional_or_answered_exit_leaves_later_steps_reachable() {
        let mut library = Script::new();
        library.insert(
            "hub",
            [
                Step::when(Requirement::has(Key::Ring), Step::stop()),
                // The host may answer this with a jump, or with Done.
                Step::act(Action::Choose(vec!["hub".into()])),
                Step::act(Action::Say("Reachable.")),
            ],
        );
        assert_eq!(problems(&library), vec![]);
    }

    #[test]
    fn a_warning_prints_where_and_what() {
        let warning = Warning {
            sequence: "hub".into(),
            index: 3,
            problem: Problem::MissingSequence("refused".into()),
        };
        assert_eq!(
            warning.to_string(),
            "hub[3]: names missing sequence 'refused'"
        );
    }
}
