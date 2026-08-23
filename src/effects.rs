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
}
