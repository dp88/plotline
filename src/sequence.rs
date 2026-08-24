//! Sequences and their storage.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::context::Context;
use crate::source::{SequenceFacts, SequenceRef, SequenceSource};
use crate::step::{Progress, Step, StepFacts};

/// An ordered list of shared steps.
///
/// ```
/// use plotline::{Sequence, steps};
///
/// let sequence = Sequence::new("greeting")
///     .with_step(steps::run("Greet", |_ctx| println!("Hello.")));
/// assert_eq!(sequence.len(), 1);
/// ```
#[derive(Default)]
pub struct Sequence {
    name: String,
    steps: Vec<Box<dyn Step>>,
}

impl Sequence {
    /// Creates an empty named sequence.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Returns the sequence name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Changes the sequence name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Appends a step and returns the sequence.
    #[must_use]
    pub fn with_step(mut self, step: impl Step + 'static) -> Self {
        self.push(step);
        self
    }

    /// Appends a step.
    pub fn push(&mut self, step: impl Step + 'static) {
        self.steps.push(Box::new(step));
    }

    /// Inserts a step at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index > len`.
    pub fn insert(&mut self, index: usize, step: impl Step + 'static) {
        self.steps.insert(index, Box::new(step));
    }

    /// Removes and returns the step at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> Box<dyn Step> {
        self.steps.remove(index)
    }

    /// Returns the step at `index`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&dyn Step> {
        self.steps.get(index).map(AsRef::as_ref)
    }

    /// Returns the number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns whether the sequence is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Iterates over the steps.
    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.steps.iter())
    }
}

impl core::fmt::Debug for Sequence {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Sequence({:?}) ", self.name)?;
        let mut list = f.debug_list();
        for step in self {
            list.entry(&StepFacts::of(step).summary);
        }
        list.finish()
    }
}

impl core::ops::Index<usize> for Sequence {
    type Output = dyn Step;

    fn index(&self, index: usize) -> &Self::Output {
        self.steps[index].as_ref()
    }
}

impl<'a> IntoIterator for &'a Sequence {
    type Item = &'a dyn Step;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Extend<Box<dyn Step>> for Sequence {
    fn extend<T: IntoIterator<Item = Box<dyn Step>>>(&mut self, iter: T) {
        self.steps.extend(iter);
    }
}

impl FromIterator<Box<dyn Step>> for Sequence {
    /// Collects steps into an unnamed sequence.
    fn from_iter<T: IntoIterator<Item = Box<dyn Step>>>(iter: T) -> Self {
        Self {
            name: String::new(),
            steps: iter.into_iter().collect(),
        }
    }
}

/// Iterator over sequence steps.
pub struct Iter<'a>(core::slice::Iter<'a, Box<dyn Step>>);

impl<'a> Iterator for Iter<'a> {
    type Item = &'a dyn Step;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(AsRef::as_ref)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_> {}

/// One warning produced by [`Library::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationWarning {
    /// Sequence containing the problem.
    pub sequence: SequenceRef,
    /// Step containing the problem, or `None` for a sequence-level problem.
    pub index: Option<usize>,
    /// Human-readable warning message.
    pub message: String,
}

/// Owns sequences and creates their [`SequenceRef`] handles.
///
/// ```
/// use plotline::{Library, Sequence, steps};
///
/// let mut library = Library::new();
/// let greeting = library.insert(
///     Sequence::new("greeting").with_step(steps::run("Greet", |_ctx| {})),
/// );
/// assert_eq!(library.get(greeting).unwrap().name(), "greeting");
/// ```
#[derive(Debug, Default)]
pub struct Library {
    sequences: Vec<Sequence>,
}

impl Library {
    /// Creates an empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a sequence and returns its handle.
    pub fn insert(&mut self, sequence: Sequence) -> SequenceRef {
        self.sequences.push(sequence);
        SequenceRef::from_raw(self.sequences.len() as u64 - 1)
    }

    /// Returns the sequence behind a handle.
    #[must_use]
    pub fn get(&self, sequence: SequenceRef) -> Option<&Sequence> {
        self.sequences.get(usize::try_from(sequence.to_raw()).ok()?)
    }

    /// Returns mutable access to a sequence.
    pub fn get_mut(&mut self, sequence: SequenceRef) -> Option<&mut Sequence> {
        self.sequences
            .get_mut(usize::try_from(sequence.to_raw()).ok()?)
    }

    /// Finds the first sequence with `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<(SequenceRef, &Sequence)> {
        self.sequences
            .iter()
            .enumerate()
            .find(|(_, sequence)| sequence.name() == name)
            .map(|(index, sequence)| (SequenceRef::from_raw(index as u64), sequence))
    }

    /// Finds the handle of the first sequence with `name`.
    #[must_use]
    pub fn ref_by_name(&self, name: &str) -> Option<SequenceRef> {
        self.find(name).map(|(sequence, _)| sequence)
    }

    /// Validates names, steps, and sequence references across the library.
    ///
    /// Cycles are valid and are intentionally not reported.
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationWarning> {
        let mut warnings = Vec::new();
        for (index, sequence) in self.sequences.iter().enumerate() {
            let sequence_ref = SequenceRef::from_raw(index as u64);
            if sequence.name().trim().is_empty() {
                warnings.push(ValidationWarning {
                    sequence: sequence_ref,
                    index: None,
                    message: "Sequence has no name.".to_owned(),
                });
            } else if self
                .sequences
                .iter()
                .take(index)
                .any(|previous| previous.name() == sequence.name())
            {
                warnings.push(ValidationWarning {
                    sequence: sequence_ref,
                    index: None,
                    message: format!("Duplicate sequence name '{}'.", sequence.name()),
                });
            }

            for (step_index, step) in sequence.iter().enumerate() {
                let facts = StepFacts::of(step);
                if let Some(message) = facts.warning {
                    warnings.push(ValidationWarning {
                        sequence: sequence_ref,
                        index: Some(step_index),
                        message,
                    });
                }
                for target in facts.references {
                    if self.get(target).is_none() {
                        warnings.push(ValidationWarning {
                            sequence: sequence_ref,
                            index: Some(step_index),
                            message: format!(
                                "References missing sequence seq#{:x}.",
                                target.to_raw()
                            ),
                        });
                    }
                }
            }
        }
        warnings
    }

    /// Returns the number of sequences.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    /// Returns whether the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }

    /// Iterates over handles and sequences.
    pub fn iter(&self) -> impl Iterator<Item = (SequenceRef, &Sequence)> {
        self.sequences
            .iter()
            .enumerate()
            .map(|(i, s)| (SequenceRef::from_raw(i as u64), s))
    }
}

impl SequenceFacts for Library {
    fn step_count(&mut self, sequence: SequenceRef) -> Option<usize> {
        self.get(sequence).map(Sequence::len)
    }

    fn step_facts(&mut self, sequence: SequenceRef, index: usize) -> Option<StepFacts> {
        self.get(sequence)?.get(index).map(StepFacts::of)
    }

    fn name(&mut self, sequence: SequenceRef) -> String {
        match self.get(sequence) {
            Some(s) if !s.name().is_empty() => s.name().to_owned(),
            _ => format!("seq#{:x}", sequence.to_raw()),
        }
    }
}

impl SequenceSource for Library {
    fn start_step(
        &mut self,
        sequence: SequenceRef,
        index: usize,
        ctx: &mut Context<'_>,
    ) -> Option<Progress> {
        Some(self.get(sequence)?.get(index)?.start(ctx))
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;
    use alloc::format;

    use super::*;
    use crate::steps;
    use alloc::vec;
    use alloc::vec::Vec;

    fn logs(name: &str, lines: &[&str]) -> Sequence {
        let mut sequence = Sequence::new(name);
        for line in lines {
            sequence.push(steps::Note {
                message: (*line).to_owned(),
            });
        }
        sequence
    }

    #[test]
    fn build_index_and_iterate() {
        let sequence = logs("s", &["a", "b", "c"]);
        assert_eq!(sequence.len(), 3);
        assert!(!sequence.is_empty());
        assert_eq!(sequence[1].summary(), "Note \"b\"");
        let summaries: Vec<_> = sequence.iter().map(Step::summary).collect();
        assert_eq!(summaries.len(), 3);
        assert_eq!(sequence.iter().len(), 3);
    }

    #[test]
    fn insert_and_remove_shift_neighbours() {
        let mut sequence = logs("s", &["a", "c"]);
        sequence.insert(
            1,
            steps::Note {
                message: "b".into(),
            },
        );
        assert_eq!(sequence[1].summary(), "Note \"b\"");
        let removed = sequence.remove(0);
        assert_eq!(removed.summary(), "Note \"a\"");
        assert_eq!(sequence.len(), 2);
    }

    #[test]
    fn extend_and_collect() {
        let mut sequence = Sequence::new("s");
        sequence.extend(vec![Box::new(steps::Stop) as Box<dyn Step>]);
        assert_eq!(sequence.len(), 1);

        let collected: Sequence = vec![Box::new(steps::Stop) as Box<dyn Step>]
            .into_iter()
            .collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected.name(), "");
    }

    #[test]
    fn debug_shows_summaries() {
        let sequence = logs("greeting", &["hello"]);
        let rendered = format!("{sequence:?}");
        assert!(rendered.contains("greeting"));
        assert!(rendered.contains("hello"));
    }

    #[test]
    fn library_handles_resolve_and_strangers_do_not() {
        let mut library = Library::new();
        let a = library.insert(logs("a", &["1"]));
        let b = library.insert(logs("b", &[]));
        assert_eq!(library.get(a).unwrap().name(), "a");
        assert_eq!(library.get(b).unwrap().name(), "b");
        assert!(library.get(SequenceRef::from_raw(99)).is_none());
        assert_eq!(library.len(), 2);
        assert_eq!(library.iter().count(), 2);
        assert_eq!(library.find("b").map(|(handle, _)| handle), Some(b));
        assert_eq!(library.ref_by_name("a"), Some(a));
        assert!(library.find("missing").is_none());
    }

    #[test]
    fn library_answers_facts() {
        let mut library = Library::new();
        let a = library.insert(logs("a", &["1", "2"]));
        assert_eq!(library.step_count(a), Some(2));
        assert_eq!(library.step_facts(a, 0).unwrap().summary, "Note \"1\"");
        assert!(library.step_facts(a, 9).is_none());
        assert_eq!(library.name(a), "a");
    }

    #[test]
    fn library_validation_reports_authoring_problems() {
        let mut library = Library::new();
        let broken = library.insert(
            Sequence::new("")
                .with_step(steps::Call::to(SequenceRef::from_raw(99)))
                .with_step(steps::SetFlag::default()),
        );
        let duplicate = library.insert(Sequence::new("duplicate"));
        library.insert(Sequence::new("duplicate"));

        let warnings = library.validate();
        assert!(warnings.iter().any(|warning| {
            warning.sequence == broken
                && warning.index.is_none()
                && warning.message == "Sequence has no name."
        }));
        assert!(warnings.iter().any(|warning| {
            warning.sequence == broken
                && warning.index == Some(0)
                && warning.message.contains("missing sequence")
        }));
        assert!(warnings.iter().any(|warning| {
            warning.sequence == broken
                && warning.index == Some(1)
                && warning.message == "No flag name set."
        }));
        assert!(warnings.iter().any(|warning| {
            warning.sequence != duplicate && warning.message.contains("Duplicate sequence name")
        }));
    }
}
