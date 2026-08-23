//! Reachability analysis: which steps can run, where a sequence certainly ends.
//!
//! This is editor infrastructure that lives in the headless crate on purpose — the
//! questions ("is this step reachable?", "does this branch resolve?") are about the data,
//! not about any editor, so they are answerable and testable without one.

use std::collections::HashSet;

use crate::source::{SequenceFacts, SequenceRef};
use crate::step::Flow;

/// The shape a rail drawing gives one step's node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailShape {
    /// A plain step: execution flows through it.
    Circle,
    /// A control-flow step: it ends the sequence, or hands control to another one.
    Diamond,
}

/// Everything a rail drawing needs to know about one step's row, in one answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct RailNode {
    /// Circle for plain steps, diamond for control flow — keyed on the *declared* flow,
    /// so a disabled branch still looks like a branch.
    pub shape: RailShape,
    /// Solid when the step will certainly run as declared; hollow when it is disabled,
    /// missing, or delegates to another sequence (unproven).
    pub solid: bool,
    /// This step is the sequence's certain ending: draw the cap.
    pub terminal: bool,
    /// This step sits after the certain ending: it can never run, and is severed from
    /// the spine rather than merely dimmed.
    pub severed: bool,
    /// The line *below* this node is softened: whether execution passes it is unproven.
    /// Keyed on the *resolved* flow — deliberately asymmetric with `solid`, preserved
    /// from the original: the node says "unproven as declared", the line says "resolved".
    pub soften_below: bool,
}

/// What a step contributes to "does this sequence end here?".
///
/// A separate vocabulary from [`Flow`] on purpose: a step *declares* whether it ends, and
/// analysis *resolves* what that plus its delegate actually amounts to. Two questions,
/// asked at two different times, so two types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Execution certainly passes this step.
    Continues,
    /// Undecidable: the answer lives behind a delegate that is unassigned,
    /// unresolvable, or part of a cycle.
    MayEnd,
    /// Execution certainly stops here.
    Ends,
}

/// One analysed step row.
struct Row {
    declared: Flow,
    delegates: Option<SequenceRef>,
    resolved: Reach,
    enabled: bool,
    missing: bool,
    warning: Option<String>,
}

/// Flow analysis of one sequence: reachability, the certain ending, and collected
/// warnings. Rebuild it when the sequence changes; query it as often as drawing likes.
///
/// The rules, verbatim from the original:
///
/// - A **disabled** step is skipped at run time, so it cannot end anything: it continues
///   whatever it declares.
/// - A step with a [`delegates_to`] target is resolved through it: it certainly ends if
///   any enabled step of the target certainly ends (the subroutine shares the caller's
///   context, so its ending ends the caller too), certainly continues if none can, and
///   stays undecided otherwise — including through a cycle, which a visiting set refuses
///   to enter twice.
/// - The **terminal** is the first enabled step that certainly ends; everything after it
///   is **severed** — it can never run.
///
/// [`delegates_to`]: crate::Step::delegates_to
pub struct FlowModel {
    rows: Vec<Row>,
    terminal: Option<usize>,
}

impl FlowModel {
    /// Analyses `sequence` as it stands right now.
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
                        let mut visiting = HashSet::from([sequence]);
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

    /// Resolves what a delegate target contributes: [`Reach::Ends`] if any of its enabled
    /// steps certainly ends, [`Reach::MayEnd`] while anything stays undecided (an
    /// unassigned target, an unresolvable handle, a cycle), else [`Reach::Continues`].
    fn resolve_delegate(
        source: &mut dyn SequenceFacts,
        target: Option<SequenceRef>,
        visiting: &mut HashSet<SequenceRef>,
    ) -> Reach {
        let Some(target) = target else {
            return Reach::MayEnd; // nothing to chase; the claim stays a claim
        };
        if !visiting.insert(target) {
            return Reach::MayEnd; // a chain that includes itself is undecidable
        }
        let result = (|| {
            let Some(count) = source.step_count(target) else {
                return Reach::MayEnd;
            };
            let mut undecided = false;
            for index in 0..count {
                let Some(facts) = source.step_facts(target, index) else {
                    continue; // a missing step cannot end anything
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

    /// Number of analysed steps.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.rows.len()
    }

    /// Index of the first enabled step that certainly ends the sequence, if any.
    #[must_use]
    pub fn terminal_index(&self) -> Option<usize> {
        self.terminal
    }

    /// Whether this step is the certain ending.
    #[must_use]
    pub fn is_terminal(&self, index: usize) -> bool {
        self.terminal == Some(index)
    }

    /// Whether this step sits after the certain ending and can never run.
    #[must_use]
    pub fn is_severed(&self, index: usize) -> bool {
        self.terminal.is_some_and(|t| index > t)
    }

    /// Whether this step might end the sequence, without analysis being able to prove it
    /// either way.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn may_end_at(&self, index: usize) -> bool {
        self.rows[index].resolved == Reach::MayEnd
    }

    /// The step's flow as it declared it, unresolved.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn declared_flow(&self, index: usize) -> Flow {
        self.rows[index].declared
    }

    /// Whether this row is a missing step (a null slot left by a renamed class).
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    #[must_use]
    pub fn is_missing(&self, index: usize) -> bool {
        self.rows[index].missing
    }

    /// The collected warnings, in step order: each step's self-report plus a synthetic
    /// one per missing step.
    pub fn warnings(&self) -> impl Iterator<Item = (usize, &str)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| row.warning.as_deref().map(|w| (i, w)))
    }

    /// Whether this step has a warning.
    #[must_use]
    pub fn has_warning_at(&self, index: usize) -> bool {
        self.rows
            .get(index)
            .is_some_and(|row| row.warning.is_some())
    }

    /// One answer per row for a rail drawing.
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
    use super::*;
    use crate::context::Context;
    use crate::sequence::{Library, Sequence};
    use crate::step::{Progress, Step};
    use crate::steps;

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
        // "A disabled step is skipped at run time, so it cannot end anything."
        let mut library = Library::new();
        let a = library.insert(
            Sequence::new("a")
                .with_step(Disabled(steps::Stop))
                .with_step(log()),
        );
        let model = FlowModel::analyse(&mut library, a);
        assert_eq!(model.terminal_index(), None);
        assert!(!model.is_severed(1));
        // The declared flow still shows: a disabled Stop draws as a hollow diamond.
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
        // An unassigned Call delegates nowhere, so there is nothing it could end. This
        // matches what it does at run time: warn, skip, carry on.
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
        // a → calls b → calls c → Stop: the End propagates all the way up.
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
