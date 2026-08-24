//! Built-in effects.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;

use crate::vocab::{Effect, EffectCtx};

/// Sets a chain flag. Outside a chain, it does nothing.
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
    /// Creates a flag-setting effect.
    #[must_use]
    pub fn new(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    /// Creates an effect that sets a flag.
    #[must_use]
    pub fn set(name: impl Into<String>) -> Self {
        Self::new(name, true)
    }

    /// Creates an effect that clears a flag.
    #[must_use]
    pub fn clear(name: impl Into<String>) -> Self {
        Self::new(name, false)
    }
}

/// Creates an effect that sets a flag.
#[must_use]
pub fn set_flag(name: impl Into<String>) -> SetFlag {
    SetFlag::set(name)
}

/// Creates an effect that clears a flag.
#[must_use]
pub fn clear_flag(name: impl Into<String>) -> SetFlag {
    SetFlag::clear(name)
}

impl Effect for SetFlag {
    fn summary(&self) -> String {
        format!("Set flag '{}' to {}", self.name, self.value)
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No flag name set.".to_owned())
    }

    fn apply(&self, effect_ctx: &mut EffectCtx<'_>) {
        if let Some(chain) = effect_ctx.chain.as_deref_mut() {
            chain.set_flag(self.name.clone(), self.value);
        }
    }
}

/// An effect created from a closure.
pub struct Run<F> {
    name: String,
    body: F,
}

/// Wraps a closure as an effect.
///
/// ```
/// use plotline::{Effect, EffectCtx, TypeMap, effects};
/// use core::cell::Cell;
/// use std::rc::Rc;
///
/// let applied = Rc::new(Cell::new(false));
/// let seen = applied.clone();
/// let effect = effects::run("Mark applied", move |_ctx| seen.set(true));
/// let mut caps = TypeMap::new();
/// effect.apply(&mut EffectCtx {
///     target: None,
///     chain: None,
///     caps: &mut caps,
/// });
/// assert!(applied.get());
/// ```
pub fn run<F>(name: impl Into<String>, body: F) -> Run<F>
where
    F: for<'a, 'b> Fn(&'a mut EffectCtx<'b>),
{
    Run {
        name: name.into(),
        body,
    }
}

impl<F> Effect for Run<F>
where
    F: for<'a, 'b> Fn(&'a mut EffectCtx<'b>),
{
    fn summary(&self) -> String {
        self.name.clone()
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No name set; this effect is anonymous in inspectors.".to_owned())
    }

    fn apply(&self, effect_ctx: &mut EffectCtx<'_>) {
        (self.body)(effect_ctx);
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::context::{ChainFlags, TypeMap};

    #[test]
    fn set_flag_writes_the_blackboard() {
        let mut caps = TypeMap::new();
        let mut chain = ChainFlags::new();
        SetFlag {
            name: "accepted".into(),
            value: true,
        }
        .apply(&mut EffectCtx {
            target: None,
            chain: Some(&mut chain),
            caps: &mut caps,
        });
        assert!(chain.flag("accepted"));
    }

    #[test]
    fn set_flag_outside_a_chain_is_a_warned_no_op() {
        let mut caps = TypeMap::new();
        SetFlag {
            name: "accepted".into(),
            value: true,
        }
        .apply(&mut EffectCtx {
            target: None,
            chain: None,
            caps: &mut caps,
        });
    }

    #[test]
    fn empty_flag_name_warns() {
        assert!(SetFlag::default().warning().is_some());
    }

    #[test]
    fn closure_effect_reports_and_applies() {
        let applied = alloc::rc::Rc::new(core::cell::Cell::new(false));
        let seen = applied.clone();
        let effect = run("Mark applied", move |_ctx| seen.set(true));
        assert_eq!(effect.summary(), "Mark applied");
        assert!(effect.warning().is_none());

        let mut caps = TypeMap::new();
        effect.apply(&mut EffectCtx {
            target: None,
            chain: None,
            caps: &mut caps,
        });
        assert!(applied.get());
        assert!(run("", |_ctx| {}).warning().is_some());
    }
}
