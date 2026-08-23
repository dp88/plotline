//! Sequence reachability analysis.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use alloc::collections::BTreeSet;

use crate::source::{SequenceFacts, SequenceRef};
use crate::step::Flow;

/// Shape of an analysis node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailShape {
    /// Execution flows through the step.
    Circle,
    /// The step ends or delegates execution.
    Diamond,
}

/// Analysis data for one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct RailNode {
    /// Node shape from the declared flow.
    pub shape: RailShape,
    /// Whether the step is enabled, present, and not delegated.
    pub solid: bool,
    /// Whether this step is the certain ending.
    pub terminal: bool,
    /// Whether this step follows the certain ending.
    pub severed: bool,
    /// Whether execution below this step is uncertain.
    pub soften_below: bool,
}

/// Resolved sequence reachability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Execution continues.
    Continues,
    /// The result is uncertain.
    MayEnd,
    /// Execution ends.
    Ends,
}

/// Internal row data.
struct Row {
    declared: Flow,
    delegates: Option<SequenceRef>,
    resolved: Reach,
    enabled: bool,
    missing: bool,
    warning: Option<String>,
}

/// Reachability analysis for one sequence.
pub struct FlowModel {
    rows: Vec<Row>,
    terminal: Option<usize>,
}

impl FlowModel {
    /// Analyses the sequence as it stands now.
    pub fn analyse(source: &mut dyn SequenceFacts, sequence: SequenceRef) -> Self {
        let count = source.step_count(sequence).unwrap_or(0);
        let mut rows = Vec::with_capacity(count);
        for index in 0..count {
            rows.push(match source.step_facts(sequence, index) {
                None => Row {
                    declared: Flow::Continue,
                    delegates: None,
                    resolved: Reach::Continues,
                    enabled: true,
                    missing: true,
                    warning: Some("Missing step (class renamed or removed?)".to_owned()),
                },
                Some(facts) => {
                    let declared = facts.flow;
                    let resolved = if !facts.enabled {
                        Reach::Continues
                    } else if declared == Flow::End {
                        Reach::Ends
                    } else if facts.delegates_to.is_some() {
                        let mut visiting = BTreeSet::from([sequence]);
                        Self::resolve_delegate(source, facts.delegates_to, &mut visiting)
                    } else {
                        Reach::Continues
                    };
                    Row {
                        declared,
                        delegates: facts.delegates_to,
                        resolved,
                        enabled: facts.enabled,
                        missing: false,
                        warning: facts.warning,
                    }
                }
            });
        }
        let terminal = rows
            .iter()
            .position(|row| row.enabled && !row.missing && row.resolved == Reach::Ends);
        Self { rows, terminal }
    }

    /// Resolves a delegated sequence.
    fn resolve_delegate(
        source: &mut dyn SequenceFacts,
        target: Option<SequenceRef>,
        visiting: &mut BTreeSet<SequenceRef>,
    ) -> Reach {
        let Some(target) = target else {
            return Reach::MayEnd;
        };
        if !visiting.insert(target) {
            return Reach::MayEnd;
        }
        let result = (|| {
            let Some(count) = source.step_count(target) else {
                return Reach::MayEnd;
            };
            let mut undecided = false;
            for index in 0..count {
                let Some(facts) = source.step_facts(target, index) else {
                    continue;
                };
                if !facts.enabled {
                    continue;
                }
                if facts.flow == Flow::End {
                    return Reach::Ends;
                }
                if facts.delegates_to.is_some() {
                    match Self::resolve_delegate(source, facts.delegates_to, visiting) {
                        Reach::Ends => return Reach::Ends,
                        Reach::MayEnd => undecided = true,
                        Reach::Continues => {}
                    }
                }
            }
            if undecided {
                Reach::MayEnd
            } else {
                Reach::Continues
            }
        })();
        visiting.remove(&target);
        result
    }

    /// Returns the number of analysed steps.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns the first certain terminal index.
    #[must_use]
    pub fn terminal_index(&self) -> Option<usize> {
        self.terminal
    }

    /// Returns whether the step is terminal.
    #[must_use]
    pub fn is_terminal(&self, index: usize) -> bool {
        self.terminal == Some(index)
    }

    /// Returns whether the step is severed.
    #[must_use]
    pub fn is_severed(&self, index: usize) -> bool {
        self.terminal.is_some_and(|t| index > t)
    }

    /// Returns whether the step may end the sequence.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn may_end_at(&self, index: usize) -> bool {
        self.rows[index].resolved == Reach::MayEnd
    }

    /// Returns the declared flow.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn declared_flow(&self, index: usize) -> Flow {
        self.rows[index].declared
    }

    /// Returns whether the step is missing.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn is_missing(&self, index: usize) -> bool {
        self.rows[index].missing
    }

    /// Iterates over warnings in step order.
    pub fn warnings(&self) -> impl Iterator<Item = (usize, &str)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| row.warning.as_deref().map(|w| (i, w)))
    }

    /// Returns whether the step has a warning.
    #[must_use]
    pub fn has_warning_at(&self, index: usize) -> bool {
        self.rows
            .get(index)
            .is_some_and(|row| row.warning.is_some())
    }

    /// Returns the node for one row.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn node(&self, index: usize) -> RailNode {
        let row = &self.rows[index];
        RailNode {
            shape: if row.declared == Flow::End || row.delegates.is_some() {
                RailShape::Diamond
            } else {
                RailShape::Circle
            },
            solid: row.enabled && !row.missing && row.delegates.is_none(),
            terminal: self.is_terminal(index),
            severed: self.is_severed(index),
            soften_below: row.resolved == Reach::MayEnd,
        }
    }
}

#[cfg(test)]
mod tests {

    use alloc::string::String;

    use super::*;
    use crate::context::Context;
    use crate::sequence::{Library, Sequence};
    use crate::step::{Progress, Step};
    use crate::steps;
    use alloc::vec::Vec;

    /// Wraps a step and reports it disabled — the headless stand-in for the authoring
    /// toggle.
    struct Disabled<S: Step>(S);

    impl<S: Step> Step for Disabled<S> {
        fn summary(&self) -> String {
            self.0.summary()
        }
        fn warning(&self) -> Option<String> {
            self.0.warning()
        }
        fn flow(&self) -> Flow {
            self.0.flow()
        }
        fn delegates_to(&self) -> Option<SequenceRef> {
            self.0.delegates_to()
        }
        fn is_enabled(&self) -> bool {
            false
        }
        fn start(&self, ctx: &mut Context<'_>) -> Progress {
            self.0.start(ctx)
        }
    }

    fn log() -> steps::Note {
        steps::Note {
            message: "x".into(),
        }
    }

    #[test]
    fn a_plain_sequence_has_no_terminal() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(log()).with_step(log()));
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.step_count(), 2);
        assert_eq!(model.terminal_index(), None);
        assert!(!model.is_severed(1));
    }

    #[test]
    fn terminal_severs_everything_below_it() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(log())
                .with_step(steps::Stop)
                .with_step(log()),
        );
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.terminal_index(), Some(1));
        assert!(model.is_terminal(1));
        assert!(!model.is_severed(0));
        assert!(model.is_severed(2));
        assert!(model.node(1).terminal);
        assert!(model.node(2).severed);
    }

    #[test]
    fn disabled_step_cannot_end_anything() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(Disabled(steps::Stop))
                .with_step(log()),
        );
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.terminal_index(), None);
        assert!(!model.is_severed(1));
        let node = model.node(0);
        assert_eq!(node.shape, RailShape::Diamond);
        assert!(!node.solid);
    }

    #[test]
    fn may_end_promoted_when_delegate_certainly_ends() {
        let mut library = Library::new();
        let ends = library.insert(Sequence::new("ends").with_step(steps::Stop));
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Call {
                    sequence: Some(ends),
                })
                .with_step(log()),
        );
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(
            model.terminal_index(),
            Some(0),
            "the subroutine shares the caller's context, so its ending ends us too"
        );
        assert!(model.is_severed(1));
        let node = model.node(0);
        assert_eq!(node.shape, RailShape::Diamond, "declared MayEnd");
        assert!(
            !node.solid,
            "hollow: the declaration was unproven by itself"
        );
        assert!(!node.soften_below, "resolved to a certainty");
    }

    #[test]
    fn may_end_demoted_when_delegate_only_continues() {
        let mut library = Library::new();
        let harmless = library.insert(Sequence::new("harmless").with_step(log()));
        let a = library.insert(
            Sequence::new("a")
                .with_step(steps::Call {
                    sequence: Some(harmless),
                })
                .with_step(log()),
        );
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.terminal_index(), None);
        assert!(
            !model.may_end_at(0),
            "demoted: the target provably continues"
        );
        assert!(!model.node(0).soften_below);
    }

    #[test]
    fn a_call_with_no_target_resolves_to_continue() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(steps::Call::default()));
        let model = FlowModel::analyse(&mut library, a);
        assert!(!model.may_end_at(0));
        assert!(!model.node(0).soften_below);
        assert_eq!(
            model.node(0).shape,
            RailShape::Circle,
            "nothing to delegate to and no end declared"
        );
    }

    #[test]
    fn may_end_stays_undecided_through_a_cycle() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a"));
        library
            .get_mut(a)
            .unwrap()
            .push(steps::Call { sequence: Some(a) });
        let model = FlowModel::analyse(&mut library, a);
        assert!(
            model.may_end_at(0),
            "a subroutine chain that includes itself is undecidable, not a hang"
        );
        assert_eq!(model.terminal_index(), None);
    }

    #[test]
    fn delegate_chase_follows_nested_calls() {
        let mut library = Library::new();
        let c = library.insert(Sequence::new("c").with_step(steps::Stop));
        let b = library.insert(Sequence::new("b").with_step(steps::Call { sequence: Some(c) }));
        let a = library.insert(Sequence::new("a").with_step(steps::Call { sequence: Some(b) }));
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.terminal_index(), Some(0));
    }

    #[test]
    fn step_warnings_surface_in_the_model() {
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(log())
                .with_step(steps::Call::default()), // "No sequence assigned…"
        );
        let model = FlowModel::analyse(&mut library, a);
        let warnings: Vec<_> = model.warnings().collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].0, 1);
        assert!(model.has_warning_at(1));
        assert!(!model.has_warning_at(0));
    }

    #[test]
    fn branch_is_a_solid_terminal_diamond() {
        let mut library = Library::new();
        let a = library.insert(Sequence::new("a").with_step(steps::Branch::default()));
        let model = FlowModel::analyse(&mut library, a);
        let node = model.node(0);
        assert_eq!(node.shape, RailShape::Diamond);
        assert!(node.solid, "End is certain, not a MayEnd claim");
        assert!(node.terminal);
    }
}
