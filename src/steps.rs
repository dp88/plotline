//! Built-in steps.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::context::Context;
use crate::source::SequenceRef;
use crate::step::{Flow, IntoProgress, Progress, Step};
use crate::vocab::{Condition, Effect};

/// Adds a note to the runner event stream.
#[derive(Clone, Debug, Default)]
pub struct Note {
    /// Note text.
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

/// Sets a chain flag.
#[derive(Clone, Debug)]
pub struct SetFlag {
    /// Flag name.
    pub name: String,
    /// Value to set.
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

impl SetFlag {
    /// Creates a flag-setting step.
    #[must_use]
    pub fn new(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    /// Creates a step that sets a flag.
    #[must_use]
    pub fn set(name: impl Into<String>) -> Self {
        Self::new(name, true)
    }

    /// Creates a step that clears a flag.
    #[must_use]
    pub fn clear(name: impl Into<String>) -> Self {
        Self::new(name, false)
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

/// Chooses a target and ends the current sequence.
#[derive(Default)]
pub struct Branch {
    /// Condition for selecting a target.
    pub condition: Option<Box<dyn Condition>>,
    /// Target when the condition is true.
    pub if_true: Option<SequenceRef>,
    /// Target when the condition is false.
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

/// Creates a conditional branch.
#[must_use]
pub fn branch(
    condition: impl Condition + 'static,
    if_true: Option<SequenceRef>,
    if_false: Option<SequenceRef>,
) -> Branch {
    Branch {
        condition: Some(Box::new(condition)),
        if_true,
        if_false,
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
        if let Some(condition) = &self.condition {
            if let Some(warning) = condition.warning() {
                return Some(format!("Condition: {warning}"));
            }
            if self.if_true == self.if_false {
                return Some(
                    "Both outcomes lead to the same place; the condition decides nothing."
                        .to_owned(),
                );
            }
        }
        None
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

/// Runs a step only when a condition is true.
pub struct When {
    /// Condition that controls execution.
    pub condition: Box<dyn Condition>,
    /// Step to run when the condition is true.
    pub step: Box<dyn Step>,
}

/// Creates a conditional step.
#[must_use]
pub fn when(condition: impl Condition + 'static, step: impl Step + 'static) -> When {
    When {
        condition: Box::new(condition),
        step: Box::new(step),
    }
}

impl Step for When {
    fn summary(&self) -> String {
        format!(
            "When ({}) → {}",
            self.condition.summary(),
            self.step.summary()
        )
    }

    fn warning(&self) -> Option<String> {
        self.condition
            .warning()
            .map(|warning| format!("Condition: {warning}"))
            .or_else(|| {
                self.step
                    .warning()
                    .map(|warning| format!("Step: {warning}"))
            })
    }

    fn flow(&self) -> Flow {
        self.step.flow()
    }

    fn delegates_to(&self) -> Option<SequenceRef> {
        self.step.delegates_to()
    }

    fn is_enabled(&self) -> bool {
        self.step.is_enabled()
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        if ctx.eval(self.condition.as_ref()) {
            self.step.start(ctx)
        } else {
            Progress::Done
        }
    }
}

/// Ends the current chain and optionally starts another sequence.
#[derive(Clone, Copy, Debug, Default)]
pub struct Goto {
    /// Sequence to start, or `None` to finish the chain.
    pub sequence: Option<SequenceRef>,
}

impl Goto {
    /// Creates a jump to `sequence`.
    #[must_use]
    pub const fn to(sequence: SequenceRef) -> Self {
        Self {
            sequence: Some(sequence),
        }
    }

    /// Creates a step that finishes the chain.
    #[must_use]
    pub const fn end() -> Self {
        Self { sequence: None }
    }
}

/// Creates a step that jumps to `sequence`.
#[must_use]
pub const fn goto(sequence: SequenceRef) -> Goto {
    Goto::to(sequence)
}

impl Step for Goto {
    fn summary(&self) -> String {
        format!("Goto {}", Branch::target_name(self.sequence))
    }

    fn flow(&self) -> Flow {
        Flow::End
    }

    fn delegates_to(&self) -> Option<SequenceRef> {
        self.sequence
    }

    fn start(&self, _ctx: &mut Context<'_>) -> Progress {
        Progress::Goto(self.sequence)
    }
}

/// Runs another sequence as a subroutine.
#[derive(Clone, Copy, Debug, Default)]
pub struct Call {
    /// Sequence to run.
    pub sequence: Option<SequenceRef>,
}

impl Call {
    /// Creates a call to `sequence`.
    #[must_use]
    pub const fn to(sequence: SequenceRef) -> Self {
        Self {
            sequence: Some(sequence),
        }
    }
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

/// Returns from the current subroutine.
#[derive(Clone, Copy, Debug, Default)]
pub struct Return;

impl Step for Return {
    fn summary(&self) -> String {
        "Return".to_owned()
    }

    fn flow(&self) -> Flow {
        Flow::End
    }

    fn start(&self, _ctx: &mut Context<'_>) -> Progress {
        Progress::Return
    }
}

/// Ends the chain immediately.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stop;

/// Creates a step that stops the chain.
#[must_use]
pub const fn stop() -> Stop {
    Stop
}

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

/// Applies effects in order.
#[derive(Default)]
pub struct ApplyEffects {
    /// Effects to apply.
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
        if self.effects.is_empty() {
            return Some("No effects to apply.".to_owned());
        }
        self.effects.iter().enumerate().find_map(|(index, effect)| {
            effect
                .warning()
                .map(|warning| format!("Effect {index}: {warning}"))
        })
    }

    fn start(&self, ctx: &mut Context<'_>) -> Progress {
        ctx.enact(self.effects.iter().map(AsRef::as_ref));
        Progress::Done
    }
}

/// A step created from a closure.
pub struct Run<F> {
    name: String,
    flow: Flow,
    delegates_to: Option<SequenceRef>,
    body: F,
}

/// Wraps a closure as a step. The body can return `()`, [`Completion`](crate::Completion),
/// or [`Progress`].
///
/// ```
/// use plotline::{Sequence, steps};
///
/// let sequence = Sequence::new("greeting")
///     .with_step(steps::run("Greet", |_ctx| println!("Hello.")));
/// assert_eq!(sequence[0].summary(), "Greet");
/// ```
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
    /// Declares that no later step runs.
    #[must_use]
    pub fn ends(mut self) -> Self {
        self.flow = Flow::End;
        self
    }

    /// Declares the delegated sequence.
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

    use alloc::boxed::Box;

    use alloc::vec;

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
    fn branch_propagates_condition_warnings() {
        let branch = Branch {
            condition: Some(Box::new(conditions::Not::default())),
            if_true: Some(SequenceRef::from_raw(1)),
            if_false: None,
        };
        assert!(branch.warning().unwrap().contains("Condition:"));
    }

    #[test]
    fn constructors_build_common_steps() {
        let target = SequenceRef::from_raw(3);
        let branch = branch(conditions::Always::default(), Some(target), None);
        assert_eq!(branch.if_true, Some(target));
        assert_eq!(Call::to(target).sequence, Some(target));
        assert_eq!(goto(target).sequence, Some(target));
        assert_eq!(SetFlag::set("accepted").value, true);
        assert_eq!(SetFlag::clear("accepted").value, false);
        assert_eq!(stop().summary(), "Stop");
    }

    #[test]
    fn when_skips_the_inner_step_when_false() {
        let step = when(conditions::Always { value: false }, Stop);
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Done));
    }

    #[test]
    fn when_runs_the_inner_step_when_true() {
        let step = when(conditions::Always::default(), Stop);
        assert_eq!(step.flow(), Flow::End);
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Goto(None)));
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
    fn return_ends_the_current_subroutine() {
        let (progress, _) = with_ctx(|ctx| Return.start(ctx));
        assert!(matches!(progress, Progress::Return));
        assert_eq!(Return.flow(), Flow::End);
    }

    #[test]
    fn stop_ends_the_chain() {
        let (progress, _) = with_ctx(|ctx| Stop.start(ctx));
        assert!(matches!(progress, Progress::Goto(None)));
    }

    #[test]
    fn goto_jumps_to_a_sequence_and_reports_end_flow() {
        let target = SequenceRef::from_raw(8);
        let step = goto(target);
        assert_eq!(step.summary(), "Goto seq#8");
        assert_eq!(step.flow(), Flow::End);
        assert_eq!(step.delegates_to(), Some(target));
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
        assert!(matches!(progress, Progress::Goto(Some(r)) if r == target));
    }

    #[test]
    fn goto_end_finishes_the_chain() {
        let step = Goto::end();
        let (progress, _) = with_ctx(|ctx| step.start(ctx));
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
    fn apply_effects_propagates_effect_warnings() {
        let effects = ApplyEffects {
            effects: vec![Box::new(crate::effects::SetFlag::default())],
        };
        assert!(effects.warning().unwrap().contains("Effect 0:"));
    }

    #[test]
    fn a_closure_step_needs_no_type_annotation() {
        // Verify that closure argument types are inferred.
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
