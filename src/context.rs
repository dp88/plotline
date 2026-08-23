//! What a running step may touch: chain state, services, and the world seams.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::vocab::{Condition, Effect, EffectCtx, QueryCtx};

/// A small typed registry: one value per type.
///
/// Used in two roles. As the runner's **service registry** it replaces the C# original's
/// static service slots — a dialog presenter, a timer service — without a central list to
/// edit when a new service appears. As the **capability set** of an [`EffectCtx`] it is
/// how call sites offer whatever they have (a stat block wrapper, an inventory handle)
/// without this crate ever naming a game type.
///
/// # Examples
///
/// ```
/// use plotline::TypeMap;
///
/// struct Greeting(String);
///
/// let mut map = TypeMap::new();
/// map.insert(Greeting("hello".into()));
/// assert_eq!(map.get::<Greeting>().unwrap().0, "hello");
/// assert!(map.get::<u32>().is_none());
/// ```
#[derive(Default)]
pub struct TypeMap {
    entries: HashMap<TypeId, Box<dyn Any>>,
}

impl TypeMap {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a value under its type, returning the previous value of that type if one
    /// was present.
    pub fn insert<T: Any>(&mut self, value: T) -> Option<T> {
        self.entries
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    /// The stored value of this type, if any.
    #[must_use]
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Mutable access to the stored value of this type, if any.
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.entries
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>())
    }

    /// Removes and returns the stored value of this type, if any.
    pub fn remove<T: Any>(&mut self) -> Option<T> {
        self.entries
            .remove(&TypeId::of::<T>())
            .and_then(|old| old.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }
}

impl std::fmt::Debug for TypeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeMap")
            .field("entries", &self.entries.len())
            .finish()
    }
}

/// The chain-local blackboard: named boolean flags.
///
/// Lives for a whole chain, not one sequence — a branch hops to a new sequence
/// mid-conversation, and the blackboard has to survive that hop or the choice that caused
/// the branch is forgotten by the sequence that handles it. An unset flag reads `false`.
#[derive(Debug, Default)]
pub struct ChainFlags {
    flags: HashMap<String, bool>,
}

impl ChainFlags {
    /// An empty blackboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value of a flag; unset flags read `false`.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    /// Sets a flag for later conditions and branches to read.
    pub fn set_flag(&mut self, name: impl Into<String>, value: bool) {
        self.flags.insert(name.into(), value);
    }
}

/// Everything the runner tracks for one chain. Crate-internal: steps see it only through
/// [`Context`].
#[derive(Default)]
pub(crate) struct ChainState {
    pub(crate) flags: ChainFlags,
    pub(crate) instigator: Option<Box<dyn Any>>,
}

/// What a step touches while it runs: the chain's state and the host's services.
///
/// Assembled by the runner for each step invocation; steps never store it. Steps
/// communicate *only* through this — reaching a system means finding its service here,
/// and a missing service must finish rather than hang.
///
/// It carries no control flow. A step redirects the chain by returning
/// [`Progress::Goto`](crate::Progress::Goto), never by poking state here.
pub struct Context<'a> {
    services: &'a mut TypeMap,
    chain: &'a mut ChainState,
}

impl<'a> Context<'a> {
    pub(crate) fn new(services: &'a mut TypeMap, chain: &'a mut ChainState) -> Self {
        Self { services, chain }
    }

    /// The host's service registry: presenters, timers, whatever the embedder installed.
    #[must_use]
    pub fn services(&self) -> &TypeMap {
        self.services
    }

    /// Mutable access to the service registry, for services with interior state.
    pub fn services_mut(&mut self) -> &mut TypeMap {
        self.services
    }

    /// Whatever started the chain — a trigger volume, an NPC, `None` for scripted starts.
    /// Opaque here; the embedder knows the concrete type and downcasts with
    /// [`instigator_as`](Context::instigator_as).
    #[must_use]
    pub fn instigator(&self) -> Option<&dyn Any> {
        self.chain.instigator.as_deref()
    }

    /// The instigator downcast to a concrete type, when both present and of that type.
    #[must_use]
    pub fn instigator_as<T: Any>(&self) -> Option<&T> {
        self.instigator().and_then(|any| any.downcast_ref::<T>())
    }

    /// The chain-local blackboard.
    #[must_use]
    pub fn flags(&self) -> &ChainFlags {
        &self.chain.flags
    }

    /// Mutable access to the chain-local blackboard.
    pub fn flags_mut(&mut self) -> &mut ChainFlags {
        &mut self.chain.flags
    }

    /// Shorthand for [`ChainFlags::flag`].
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.chain.flags.flag(name)
    }

    /// Shorthand for [`ChainFlags::set_flag`].
    pub fn set_flag(&mut self, name: impl Into<String>, value: bool) {
        self.chain.flags.set_flag(name, value);
    }

    /// Evaluates a condition with full chain access: the instigator as the target, the
    /// blackboard visible, the services offered as capabilities.
    #[must_use]
    pub fn eval(&self, condition: &dyn Condition) -> bool {
        condition.evaluate(&QueryCtx {
            target: self.chain.instigator.as_deref(),
            chain: Some(&self.chain.flags),
            caps: self.services,
        })
    }

    /// Applies effects the way the C# original did from sequences: the chain's instigator
    /// is the target, the blackboard is writable, the services double as capabilities.
    pub fn enact<'e>(&mut self, effects: impl IntoIterator<Item = &'e dyn Effect>) {
        let ChainState {
            flags, instigator, ..
        } = &mut *self.chain;
        for effect in effects {
            effect.apply(&mut EffectCtx {
                target: instigator.as_deref(),
                chain: Some(flags),
                caps: self.services,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typemap_stores_one_value_per_type() {
        let mut map = TypeMap::new();
        assert!(map.insert(7_u32).is_none());
        assert_eq!(map.insert(9_u32), Some(7)); // replacing returns the old value
        assert_eq!(map.get::<u32>(), Some(&9));
        *map.get_mut::<u32>().unwrap() += 1;
        assert_eq!(map.remove::<u32>(), Some(10));
        assert!(map.get::<u32>().is_none());
    }

    #[test]
    fn chain_flags_unset_reads_false() {
        let flags = ChainFlags::new();
        assert!(!flags.flag("accepted"));
    }

    #[test]
    fn chain_flags_set_and_read() {
        let mut flags = ChainFlags::new();
        flags.set_flag("accepted", true);
        assert!(flags.flag("accepted"));
        flags.set_flag("accepted", false);
        assert!(!flags.flag("accepted"));
    }

    #[test]
    fn context_exposes_instigator_by_downcast() {
        let mut services = TypeMap::new();
        let mut state = ChainState {
            instigator: Some(Box::new(42_i32)),
            ..ChainState::default()
        };
        let ctx = Context::new(&mut services, &mut state);
        assert_eq!(ctx.instigator_as::<i32>(), Some(&42));
        assert!(ctx.instigator_as::<String>().is_none());
    }
}
