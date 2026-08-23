//! Built-in steps. Use them module-qualified — `steps::Log`, `steps::Branch`.
//!
//! There are deliberately no labels or nested blocks, and no way into the middle of a
//! sequence: control flow is [`Branch`] (end here, continue as one of two sequences),
//! [`Call`] (run a sequence as a subroutine), and [`Stop`]. Each returns its decision as
//! a [`Progress`], so a step can be tested by what it answers. Note there is no timed wait here — time belongs to the
//! host, and a "wait N seconds" step lives in the layer that owns a clock.

use crate::context::Context;
use crate::source::SequenceRef;
use crate::step::{Flow, Progress, Step};
use crate::vocab::{Condition, Effect};

/// Writes a line to the log and continues. Sequences are data, so the log is the
/// debugger.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// The line to write.
    pub message: String,
}

impl Step for Log {
    fn summary(&self) -> String {
        format!("Log \"{}\"", self.message)
    }

    fn start(&self, _ctx: &mut Context<'_>) -> Progress {
        log::info!("{}", self.message);
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

    fn start(&self, _ctx: &mut Context<'_>) -> Progress {
        match self.sequence {
            Some(sequence) => Progress::Call(sequence),
            None => {
                log::warn!("Call step has no sequence assigned; skipping.");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions;
    use crate::context::{ChainState, TypeMap};

    fn with_ctx<R>(f: impl FnOnce(&mut Context<'_>) -> R) -> (R, ChainState) {
        let mut services = TypeMap::new();
        let mut state = ChainState::default();
        let result = f(&mut Context::new(&mut services, &mut state));
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
            effects: vec![Box::new(crate::effects::Log {
                message: "x".into(),
            })],
        };
        assert_eq!(one.summary(), "Apply: Log \"x\"");
    }
}
