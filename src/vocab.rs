//! The shared vocabulary: conditions ask, effects act.
//!
//! This — not the runner — is the interconnect between systems. A branch step evaluates a
//! [`Condition`]; later a dialog choice, a quest objective, and a trigger volume evaluate
//! the same types. Each system contributes conditions and effects about itself ("has
//! item", "quest at stage") without any of them knowing the others exist.
//!
//! Both kinds run at many call sites with very different capabilities on offer, so their
//! context types say exactly what is available — in the type, not at runtime. The missing
//! things are `None`, and the house rule for a missing requirement is: say so and do
//! nothing, never guess, never wait.

use std::any::Any;

use crate::context::{ChainFlags, TypeMap};

/// Read-side context for a [`Condition`].
///
/// `chain` is `None` when no chain is running — trigger gates evaluate conditions outside
/// sequences, and the type says so. (The C# original let a chain-flag condition
/// dereference a null context; here the compiler forces the question.)
pub struct QueryCtx<'a> {
    /// The subject of the question — the chain's instigator at sequence call sites,
    /// whatever the site is asking about elsewhere, or `None`.
    pub target: Option<&'a dyn Any>,
    /// The running chain's blackboard, or `None` outside a chain.
    pub chain: Option<&'a ChainFlags>,
    /// Capabilities the call site offers (world services, game systems).
    pub caps: &'a TypeMap,
}

/// Write-side context for an [`Effect`].
///
/// Fields mirror the read side, writable where writing is the point. Call sites fill
/// `caps` with whatever they can offer; game types never appear in this crate.
pub struct EffectCtx<'a> {
    /// Whoever the effect is aimed at — the drinker of the potion, the victim of the
    /// trap. `None` when the call site has no target.
    pub target: Option<&'a dyn Any>,
    /// The running chain's blackboard, or `None` when the effect fires outside a chain.
    pub chain: Option<&'a mut ChainFlags>,
    /// Capabilities the call site offers.
    pub caps: &'a mut TypeMap,
}

/// A yes/no question about the world.
///
/// Serialized polymorphically wherever content needs a gate; composable through
/// [`conditions::All`], [`conditions::Any`], and [`conditions::Not`].
///
/// [`conditions::All`]: crate::conditions::All
/// [`conditions::Any`]: crate::conditions::Any
/// [`conditions::Not`]: crate::conditions::Not
pub trait Condition {
    /// One line for logs and inspectors — the question, not the type name.
    fn summary(&self) -> String;

    /// One line naming an authoring problem, or `None` when the configuration is fine.
    /// Unlike the C# original, this *is* read and surfaced by tooling.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Answers the question against whatever the context offers.
    fn evaluate(&self, query: &QueryCtx<'_>) -> bool;
}

/// An instant action on the world.
///
/// Effects are *instant by definition*: anything that waits — for a dismissal, a timer, a
/// fight — is a [`Step`](crate::Step), not an effect. Holding that line is what keeps
/// effects freely usable from any call site (an item being used, a dialog choice, a quest
/// reward) without dragging the runner along.
pub trait Effect {
    /// One line for lists and logs — the action, not the type name.
    fn summary(&self) -> String;

    /// One line naming an authoring problem, or `None` when the configuration is fine.
    /// Unlike the C# original, this *is* read and surfaced by tooling.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Acts on whatever the context offers. An effect whose requirement is missing (a
    /// stat effect with no target) logs and does nothing rather than guessing.
    fn apply(&self, effect_ctx: &mut EffectCtx<'_>);
}
