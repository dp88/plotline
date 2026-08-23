//! Built-in effects. Use them module-qualified — `effects::Log`, `effects::SetFlag`.

use crate::vocab::{Effect, EffectCtx};

/// Writes a line to the log. The debugging effect: sequences are data, so the log is the
/// debugger.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// The line to write.
    pub message: String,
}

impl Effect for Log {
    fn summary(&self) -> String {
        format!("Log \"{}\"", self.message)
    }

    fn apply(&self, _effect_ctx: &mut EffectCtx<'_>) {
        log::info!("{}", self.message);
    }
}

/// Sets a chain-local blackboard flag — the interlock that lets a dialog choice steer the
/// sequence that ran it.
///
/// Fired outside a running chain there is nothing to write to: logs a warning and does
/// nothing, per the house rule for a missing requirement.
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
        match effect_ctx.chain.as_deref_mut() {
            Some(chain) => chain.set_flag(self.name.clone(), self.value),
            None => log::warn!(
                "Set-flag effect '{}' fired outside a chain; nothing to write to.",
                self.name
            ),
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
