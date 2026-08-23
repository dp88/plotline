//! The data structure itself: [`Sequence`] and the arena that owns them, [`Library`].

use crate::context::Context;
use crate::source::{SequenceFacts, SequenceRef, SequenceSource};
use crate::step::{Progress, Step, StepFacts};

/// An ordered list of steps — a plain value.
///
/// Build it, iterate it, analyse it, run it. No engine, no registry, no interior
/// mutability: a `Sequence` is immutable while it runs (all run state lives on the
/// [`Runner`](crate::Runner)), so any number of chains can execute one sequence without
/// trampling each other.
///
/// # Examples
///
/// ```
/// use plotline::{Sequence, steps};
///
/// let greeting = Sequence::new("greeting")
///     .with_step(steps::Note { message: "Hello, traveler.".into() })
///     .with_step(steps::SetFlag { name: "greeted".into(), value: true });
///
/// assert_eq!(greeting.len(), 2);
/// assert_eq!(greeting[0].summary(), "Note \"Hello, traveler.\"");
/// for step in &greeting {
///     println!("{}", step.summary());
/// }
/// ```
#[derive(Default)]
pub struct Sequence {
    name: String,
    steps: Vec<Box<dyn Step>>,
}

impl Sequence {
    /// An empty sequence with a name for logs and editors.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// The sequence's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renames the sequence.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Builder-style [`push`](Sequence::push), for reading a sequence top to bottom at
    /// the construction site.
    #[must_use]
    pub fn with_step(mut self, step: impl Step + 'static) -> Self {
        self.push(step);
        self
    }

    /// Appends a step.
    pub fn push(&mut self, step: impl Step + 'static) {
        self.steps.push(Box::new(step));
    }

    /// Inserts a step at `index`, shifting later steps down.
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

    /// The step at `index`, or `None` past the end.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&dyn Step> {
        self.steps.get(index).map(AsRef::as_ref)
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the sequence has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Iterates the steps in order.
    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.steps.iter())
    }
}

impl std::fmt::Debug for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sequence({:?}) ", self.name)?;
        let mut list = f.debug_list();
        for step in self {
            // StepFacts::of, not step.summary(): Debug on half-authored content must not
            // panic just because a summary does.
            list.entry(&StepFacts::of(step).summary);
        }
        list.finish()
    }
}

impl std::ops::Index<usize> for Sequence {
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
    /// Collects steps into an unnamed sequence; name it afterwards with
    /// [`set_name`](Sequence::set_name) if it will appear in logs.
    fn from_iter<T: IntoIterator<Item = Box<dyn Step>>>(iter: T) -> Self {
        Self {
            name: String::new(),
            steps: iter.into_iter().collect(),
        }
    }
}

/// Iterator over a sequence's steps. Created by [`Sequence::iter`].
pub struct Iter<'a>(std::slice::Iter<'a, Box<dyn Step>>);

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

/// Owns sequences and mints the [`SequenceRef`] handles steps use to reference each
/// other — the arena that answers "how do branch targets work without an engine".
///
/// `Library` is the canonical [`SequenceSource`]: tests, headless tools, and other
/// embeddings drive the [`Runner`](crate::Runner) with one of these. Handles are
/// stable for the life of the library (nothing is ever removed; authoring removal is an
/// editor concern, not a runtime one).
///
/// # Examples
///
/// ```
/// use plotline::{Library, Sequence, steps};
///
/// let mut library = Library::new();
/// let farewell = library.insert(
///     Sequence::new("farewell").with_step(steps::Note { message: "Safe roads.".into() }),
/// );
/// let greeting = library.insert(
///     Sequence::new("greeting").with_step(steps::Call { sequence: Some(farewell) }),
/// );
///
/// assert_eq!(library.get(greeting).unwrap().name(), "greeting");
/// ```
#[derive(Debug, Default)]
pub struct Library {
    sequences: Vec<Sequence>,
}

impl Library {
    /// An empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes ownership of a sequence and returns the handle other sequences use to
    /// reference it.
    pub fn insert(&mut self, sequence: Sequence) -> SequenceRef {
        self.sequences.push(sequence);
        SequenceRef::from_raw(self.sequences.len() as u64 - 1)
    }

    /// The sequence behind a handle, or `None` when the handle is past the end.
    ///
    /// A handle is an index into *this* library. A handle from another library resolves
    /// to whatever sits at the same index here, silently — so keep one library per set of
    /// sequences that reference each other.
    #[must_use]
    pub fn get(&self, sequence: SequenceRef) -> Option<&Sequence> {
        self.sequences.get(usize::try_from(sequence.to_raw()).ok()?)
    }

    /// Mutable access to the sequence behind a handle, for authoring.
    pub fn get_mut(&mut self, sequence: SequenceRef) -> Option<&mut Sequence> {
        self.sequences
            .get_mut(usize::try_from(sequence.to_raw()).ok()?)
    }

    /// Number of sequences.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    /// Whether the library holds no sequences.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }

    /// Iterates the sequences with their handles.
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
        // A Vec of boxes cannot hold a missing step, so resolution failure is the only
        // `None` here; serialized storages may also return `None` for stale or unknown
        // step records.
        Some(self.get(sequence)?.get(index)?.start(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps;

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
}
