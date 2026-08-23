//! Built-in effects. Use them module-qualified — `effects::SetFlag`.
//!
//! An effect runs far outside the runner — an item being used, a dialog choice, a trigger
//! volume — so it has no event stream to speak into. An effect reports authoring problems
//! through [`warning`](crate::Effect::warning) and otherwise stays quiet.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;

use crate::vocab::{Effect, EffectCtx};

/// Sets a chain-local blackboard flag — the interlock that lets a dialog choice steer the
/// sequence that ran it.
///
/// Fired outside a running chain there is nothing to write to, so it does nothing — the
/// house rule for a missing requirement.
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
        // No chain, nothing to write to. The `Option` on `EffectCtx::chain` is what makes
        // a caller acknowledge that; doing nothing is the house rule for a missing
        // requirement.
        if let Some(chain) = effect_ctx.chain.as_deref_mut() {
            chain.set_flag(self.name.clone(), self.value);
        }
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
        // No chain on offer: must not panic, must not invent one.
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
}
