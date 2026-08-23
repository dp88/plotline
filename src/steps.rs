//! Built-in steps. Use them module-qualified — `steps::Note`, `steps::Branch`.
//!
//! [`run`] is the short way to contribute a verb: it wraps a closure, so a one-off step
//! needs no struct and no `impl` block.
//!
//! There are deliberately no labels or nested blocks, and no way into the middle of a
//! sequence: control flow is [`Branch`] (end here, continue as one of two sequences),
//! [`Call`] (run a sequence as a subroutine), and [`Stop`]. Each returns its decision as
//! a [`Progress`], so a step can be tested by what it answers. Note there is no timed wait here — time belongs to the
//! host, and a "wait N seconds" step lives in the layer that owns a clock.

use crate::context::Context;
use crate::source::SequenceRef;
use crate::step::{Flow, IntoProgress, Progress, Step};
use crate::vocab::{Condition, Effect};

/// Says a line into the runner's event stream and continues. Sequences are data, so the
/// event stream is the debugger.
#[derive(Clone, Debug, Default)]
pub struct Note {
    /// The line to say.
    pub message: String,
}

impl Step for Note {
    fn summary(&self) -> String {
        format!("Note \"{}\"", self.message)
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        ctx.note(self.message.clone());
        Progress::Done
    }
}

/// Sets a chain-local blackboard flag for a later [`Branch`] to read.
#[derive(Clone, Debug)]
pub struct SetFlag {
    /// The flag to set.
    pub name: String,
    /// The value to set it to.
    pub value: bool,
}

impl Default for SetFlag {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: true,
        }
    }
}

impl Step for SetFlag {
    fn summary(&self) -> String {
        format!("Set flag '{}' to {}", self.name, self.value)
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No flag name set.".to_owned())
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        ctx.set_flag(self.name.clone(), self.value);
        Progress::Done
    }
}

/// Ends this sequence and continues the chain with one of two others, chosen by a
/// condition. No condition means an unconditional branch to the true target; a chosen
/// target of `None` simply ends the chain.
#[derive(Default)]
pub struct Branch {
    /// The question that picks a side; `None` branches unconditionally to
    /// [`if_true`](Branch::if_true).
    pub condition: Option<Box<dyn Condition>>,
    /// Where the chain continues when the condition holds. `None` ends the chain.
    pub if_true: Option<SequenceRef>,
    /// Where the chain continues when the condition fails. `None` ends the chain.
    pub if_false: Option<SequenceRef>,
}

impl Branch {
    fn target_name(target: Option<SequenceRef>) -> String {
        match target {
            Some(r) => format!("seq#{:x}", r.to_raw()),
            None => "(end)".to_owned(),
        }
    }
}

impl Step for Branch {
    fn summary(&self) -> String {
        match &self.condition {
            Some(c) => format!(
                "Branch ({}) → {} / {}",
                c.summary(),
                Self::target_name(self.if_true),
                Self::target_name(self.if_false),
            ),
            None => format!("Branch → {}", Self::target_name(self.if_true)),
        }
    }

    fn warning(&self) -> Option<String> {
        (self.condition.is_some() && self.if_true == self.if_false).then(|| {
            "Both outcomes lead to the same place; the condition decides nothing.".to_owned()
        })
    }

    fn flow(&self) -> Flow {
        Flow::End
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        let took_true = match &self.condition {
            Some(condition) => ctx.eval(condition.as_ref()),
            None => true,
        };
        Progress::Goto(if took_true {
            self.if_true
        } else {
            self.if_false
        })
    }
}

/// Runs another sequence as a subroutine — against the same context, so a
/// [`Stop`] or [`Branch`] inside it ends this sequence too — then continues with the
/// next step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Call {
    /// The sequence to run inline. `None` is an authoring gap: logged and skipped.
    pub sequence: Option<SequenceRef>,
}

impl Step for Call {
    fn summary(&self) -> String {
        format!("Call {}", Branch::target_name(self.sequence))
    }

    fn warning(&self) -> Option<String> {
        self.sequence
            .is_none()
            .then(|| "No sequence assigned; this step will be skipped.".to_owned())
    }

    fn delegates_to(&self) -> Option<SequenceRef> {
        self.sequence
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        match self.sequence {
            Some(sequence) => Progress::Call(sequence),
            None => {
                ctx.note("Call step has no sequence assigned; skipping.");
                Progress::Done
            }
        }
    }
}

/// Ends the sequence chain immediately.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stop;

impl Step for Stop {
    fn summary(&self) -> String {
        "Stop".to_owned()
    }

    fn flow(&self) -> Flow {
        Flow::End
    }

    fn start(&self, _ctx: &mut Context<'_>) -> Progress {
        Progress::Goto(None)
    }
}

/// Applies a list of instant effects to the world, in order, targeting the chain's
/// instigator — the best target a sequence knows today.
#[derive(Default)]
pub struct ApplyEffects {
    /// The effects to enact.
    pub effects: Vec<Box<dyn Effect>>,
}

impl Step for ApplyEffects {
    fn summary(&self) -> String {
        match self.effects.as_slice() {
            [] => "Apply no effects".to_owned(),
            [only] => format!("Apply: {}", only.summary()),
            many => format!("Apply {} effects", many.len()),
        }
    }

    fn warning(&self) -> Option<String> {
        self.effects
            .is_empty()
            .then(|| "No effects to apply.".to_owned())
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        ctx.enact(self.effects.iter().map(AsRef::as_ref));
        Progress::Done
    }
}

/// A step built from a closure — the short way to contribute a verb.
///
/// Created by [`run`]. Carries the name you gave it, so a closure step is still legible
/// to logs, inspectors, and [`FlowModel`](crate::FlowModel).
pub struct Run<F> {
    name: String,
    flow: Flow,
    delegates_to: Option<SequenceRef>,
    body: F,
}

/// Wraps a closure as a step.
///
/// The body answers with anything [`IntoProgress`] accepts: `()` to finish now, a
/// [`Completion`](crate::Completion) to wait on, or a full [`Progress`] when it needs
/// control flow.
///
/// # Examples
///
/// ```
/// use plotline::{Completion, Sequence, steps};
///
/// let gate = Completion::new();
/// let ready = gate.clone();
///
/// let sequence = Sequence::new("greeting")
///     .with_step(steps::run("Greet the elder", |_ctx| println!("Hello.")))
///     .with_step(steps::run("Wait for the panel", move |_ctx| ready.clone()))
///     .with_step(steps::run("Remember it", |ctx| ctx.set_flag("greeted", true)));
///
/// assert_eq!(sequence[0].summary(), "Greet the elder");
/// ```
///
/// The `for<'a, 'b>` bound below is load-bearing: without it the compiler cannot infer a
/// closure's argument here, and every call site would need an explicit
/// `|ctx: &mut Context<'_>|` annotation.
pub fn run<F, R>(name: impl Into<String>, body: F) -> Run<F>
where
    F: for<'a, 'b> Fn(&'a mut Context<'b>) -> R,
    R: IntoProgress,
{
    Run {
        name: name.into(),
        flow: Flow::Continue,
        delegates_to: None,
        body,
    }
}

impl<F> Run<F> {
    /// Declares that no step runs after this one — for a closure that answers
    /// [`Progress::Goto`]. Analysis reads this; the runner does not.
    #[must_use]
    pub fn ends(mut self) -> Self {
        self.flow = Flow::End;
        self
    }

    /// Declares the sequence this closure hands control to — for one that answers
    /// [`Progress::Call`] or [`Progress::Goto`]. Analysis reads this; the runner does not.
    #[must_use]
    pub fn delegating_to(mut self, sequence: SequenceRef) -> Self {
        self.delegates_to = Some(sequence);
        self
    }
}

impl<F, R> Step for Run<F>
where
    F: for<'a, 'b> Fn(&'a mut Context<'b>) -> R,
    R: IntoProgress,
{
    fn summary(&self) -> String {
        self.name.clone()
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No name set; this step is anonymous in logs and inspectors.".to_owned())
    }

    fn flow(&self) -> Flow {
        self.flow
    }

    fn delegates_to(&self) -> Option<SequenceRef> {
        self.delegates_to
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        (self.body)(ctx).into_progress()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions;
    use crate::context::{ChainState, TypeMap};
    use crate::runner::Events;
    use crate::sequence::Sequence;

    fn with_ctx<R>(f: impl FnOnce(&mut Context<'_>) -> R) -> (R, ChainState) {
        let mut services = TypeMap::new();
        let mut state = ChainState::default();
        let mut events = Events::default();
        let location = (SequenceRef::from_raw(0), 0);
        let result = f(&mut Context::new(
            &mut services,
            &mut state,
            &mut events,
            location,
        ));
        (result, state)
    }

    #[test]
    fn branch_without_condition_is_unconditional() {
        let branch = Branch {
            condition: None,
            if_true: Some(SequenceRef::from_raw(1)),
            if_false: Some(SequenceRef::from_raw(2)),
        };
        let (progress, _) = with_ctx(|ctx| branch.start(ctx));
        assert!(matches!(progress, Progress::Goto(Some(r)) if r == SequenceRef::from_raw(1)));
    }

    #[test]
    fn branch_picks_the_false_side() {
        let branch = Branch {
            condition: Some(Box::new(conditions::Always { value: false })),
            if_true: Some(SequenceRef::from_raw(1)),
            if_false: Some(SequenceRef::from_raw(2)),
        };
        let (progress, _) = with_ctx(|ctx| branch.start(ctx));
        assert!(matches!(progress, Progress::Goto(Some(r)) if r == SequenceRef::from_raw(2)));
    }

    #[test]
    fn branch_with_equal_targets_and_a_condition_warns() {
        let pointless = Branch {
            condition: Some(Box::new(conditions::Always::default())),
            if_true: Some(SequenceRef::from_raw(1)),
            if_false: Some(SequenceRef::from_raw(1)),
        };
        assert!(pointless.warning().is_some());
        assert!(
            Branch::default().warning().is_none(),
            "no condition, no warning"
        );
    }

    #[test]
    fn call_without_target_warns_and_skips() {
        let call = Call::default();
        assert!(call.warning().is_some());
        let (progress, _) = with_ctx(|ctx| call.start(ctx));
        assert!(matches!(progress, Progress::Done));
    }

    #[test]
    fn call_with_target_calls() {
        let call = Call {
            sequence: Some(SequenceRef::from_raw(7)),
        };
        assert!(call.warning().is_none());
        assert_eq!(call.delegates_to(), Some(SequenceRef::from_raw(7)));
        let (progress, _) = with_ctx(|ctx| call.start(ctx));
        assert!(matches!(progress, Progress::Call(r) if r == SequenceRef::from_raw(7)));
    }

    #[test]
    fn stop_ends_the_chain() {
        let (progress, _) = with_ctx(|ctx| Stop.start(ctx));
        assert!(matches!(progress, Progress::Goto(None)));
    }

    #[test]
    fn set_flag_reaches_the_blackboard() {
        let step = SetFlag {
            name: "accepted".into(),
            value: true,
        };
        let (_, state) = with_ctx(|ctx| step.start(ctx));
        assert!(state.flags.flag("accepted"));
    }

    #[test]
    fn apply_effects_summaries_scale() {
        assert_eq!(ApplyEffects::default().summary(), "Apply no effects");
        assert!(ApplyEffects::default().warning().is_some());
        let one = ApplyEffects {
            effects: vec![Box::new(crate::effects::SetFlag {
                name: "x".into(),
                value: true,
            })],
        };
        assert_eq!(one.summary(), "Apply: Set flag 'x' to true");
    }

    #[test]
    fn a_closure_step_needs_no_type_annotation() {
        // This test's value is that it compiles. Without the `for<'a, 'b>` bound on
        // `run`, every closure below fails with "implementation of FnOnce is not general
        // enough" and the terse form is unusable.
        let sequence = Sequence::new("s")
            .with_step(run("plain", |_ctx| {}))
            .with_step(run("touches the blackboard", |ctx| {
                ctx.set_flag("seen", true);
            }))
            .with_step(run("answers Progress", |_ctx| Progress::Done));
        assert_eq!(sequence.len(), 3);
    }

    #[test]
    fn a_closure_reports_its_name_to_tooling() {
        let step = run("Greet the elder", |_ctx| {});
        assert_eq!(step.summary(), "Greet the elder");
        assert!(step.warning().is_none());
        assert_eq!(step.flow(), Flow::Continue);
        assert!(step.delegates_to().is_none());
        assert!(
            run("", |_ctx| {}).warning().is_some(),
            "anonymous steps warn"
        );
    }

    #[test]
    fn a_closure_returning_unit_is_done() {
        let step = run("x", |_ctx| {});
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Done));
    }

    #[test]
    fn a_closure_returning_a_completion_waits() {
        let gate = crate::Completion::new();
        let step = run("x", move |_ctx| gate.clone());
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Wait(_)));
    }

    #[test]
    fn a_closure_can_declare_its_control_flow() {
        let target = SequenceRef::from_raw(3);
        let step = run("branch", move |_ctx| Progress::Goto(Some(target)))
            .ends()
            .delegating_to(target);
        assert_eq!(step.flow(), Flow::End);
        assert_eq!(step.delegates_to(), Some(target));
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Goto(Some(r)) if r == target));
    }
}
