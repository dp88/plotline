//! Conditions query state. Effects change state.

use alloc::string::String;

use core::any::Any;

use crate::context::{ChainFlags, TypeMap};

/// Read-only context for a [`Condition`].
pub struct QueryCtx<'a> {
    /// Subject of the query.
    pub target: Option<&'a dyn Any>,
    /// Chain flags, or `None` outside a chain.
    pub chain: Option<&'a ChainFlags>,
    /// Services available to the condition.
    pub caps: &'a TypeMap,
}

impl QueryCtx<'_> {
    /// Returns the query target as type `T`.
    #[must_use]
    pub fn target_as<T: Any>(&self) -> Option<&T> {
        self.target.and_then(|target| target.downcast_ref::<T>())
    }

    /// Returns the host service of type `T`.
    #[must_use]
    pub fn service<T: Any>(&self) -> Option<&T> {
        self.caps.get::<T>()
    }
}

/// Mutable context for an [`Effect`].
pub struct EffectCtx<'a> {
    /// Effect target.
    pub target: Option<&'a dyn Any>,
    /// Chain flags, or `None` outside a chain.
    pub chain: Option<&'a mut ChainFlags>,
    /// Services available to the effect.
    pub caps: &'a mut TypeMap,
}

impl EffectCtx<'_> {
    /// Returns the effect target as type `T`.
    #[must_use]
    pub fn target_as<T: Any>(&self) -> Option<&T> {
        self.target.and_then(|target| target.downcast_ref::<T>())
    }

    /// Returns the host service of type `T`.
    #[must_use]
    pub fn service<T: Any>(&self) -> Option<&T> {
        self.caps.get::<T>()
    }

    /// Returns mutable access to the host service of type `T`.
    pub fn service_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.caps.get_mut::<T>()
    }
}

/// A yes/no query.
pub trait Condition {
    /// Returns a display summary.
    fn summary(&self) -> String;

    /// Returns an authoring warning, if any.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Evaluates the query.
    fn evaluate(&self, query: &QueryCtx<'_>) -> bool;
}

/// An action that completes in one call.
pub trait Effect {
    /// Returns a display summary.
    fn summary(&self) -> String;

    /// Returns an authoring warning, if any.
    fn warning(&self) -> Option<String> {
        None
    }

    /// Applies the effect.
    fn apply(&self, effect_ctx: &mut EffectCtx<'_>);
}
