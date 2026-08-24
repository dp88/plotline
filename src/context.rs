//! Step context and typed state.

use alloc::boxed::Box;
use alloc::string::String;

use alloc::collections::BTreeMap;
use core::any::{Any, TypeId};

use crate::runner::{Events, RunnerEvent};
use crate::source::SequenceRef;
use crate::vocab::{Condition, Effect, EffectCtx, QueryCtx};

/// A registry with one value per type.
///
/// ```
/// use plotline::TypeMap;
///
/// struct Greeting(&'static str);
/// let mut services = TypeMap::new();
/// services.insert(Greeting("hello"));
/// assert_eq!(services.get::<Greeting>().unwrap().0, "hello");
/// ```
#[derive(Default)]
pub struct TypeMap {
    entries: BTreeMap<TypeId, Box<dyn Any>>,
}

impl TypeMap {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value and returns the previous value of the same type.
    pub fn insert<T: Any>(&mut self, value: T) -> Option<T> {
        self.entries
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    /// Returns the stored value of this type.
    #[must_use]
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Returns mutable access to the stored value of this type.
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.entries
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>())
    }

    /// Removes and returns the stored value of this type.
    pub fn remove<T: Any>(&mut self) -> Option<T> {
        self.entries
            .remove(&TypeId::of::<T>())
            .and_then(|old| old.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }
}

impl core::fmt::Debug for TypeMap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypeMap")
            .field("entries", &self.entries.len())
            .finish()
    }
}

/// Named boolean flags shared by all sequences in a chain.
#[derive(Debug, Default)]
pub struct ChainFlags {
    flags: BTreeMap<String, bool>,
}

impl ChainFlags {
    /// Creates an empty flag set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a flag value. An unset flag is `false`.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    /// Sets a flag value.
    pub fn set_flag(&mut self, name: impl Into<String>, value: bool) {
        self.flags.insert(name.into(), value);
    }
}

/// State owned by one running chain.
#[derive(Default)]
pub(crate) struct ChainState {
    pub(crate) flags: ChainFlags,
    pub(crate) instigator: Option<Box<dyn Any>>,
}

/// State and services available during one step call.
pub struct Context<'a> {
    services: &'a mut TypeMap,
    chain: &'a mut ChainState,
    events: &'a mut Events,
    location: (SequenceRef, usize),
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        services: &'a mut TypeMap,
        chain: &'a mut ChainState,
        events: &'a mut Events,
        location: (SequenceRef, usize),
    ) -> Self {
        Self {
            services,
            chain,
            events,
            location,
        }
    }

    /// Returns the sequence and step index.
    #[must_use]
    pub fn location(&self) -> (SequenceRef, usize) {
        self.location
    }

    /// Adds a location-tagged note to the runner event stream.
    pub fn note(&mut self, message: impl Into<String>) {
        let (sequence, index) = self.location;
        self.events.record(RunnerEvent::Note {
            sequence,
            index,
            message: message.into(),
        });
    }

    /// Returns the host service registry.
    #[must_use]
    pub fn services(&self) -> &TypeMap {
        self.services
    }

    /// Returns mutable access to the service registry.
    pub fn services_mut(&mut self) -> &mut TypeMap {
        self.services
    }

    /// Returns the host service of type `T`.
    #[must_use]
    pub fn service<T: Any>(&self) -> Option<&T> {
        self.services.get::<T>()
    }

    /// Returns mutable access to the host service of type `T`.
    pub fn service_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.services.get_mut::<T>()
    }

    /// Returns the opaque chain instigator.
    #[must_use]
    pub fn instigator(&self) -> Option<&dyn Any> {
        self.chain.instigator.as_deref()
    }

    /// Returns the instigator as type `T`.
    #[must_use]
    pub fn instigator_as<T: Any>(&self) -> Option<&T> {
        self.instigator().and_then(|any| any.downcast_ref::<T>())
    }

    /// Returns the chain flags.
    #[must_use]
    pub fn flags(&self) -> &ChainFlags {
        &self.chain.flags
    }

    /// Returns mutable access to the chain flags.
    pub fn flags_mut(&mut self) -> &mut ChainFlags {
        &mut self.chain.flags
    }

    /// Returns a chain flag.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.chain.flags.flag(name)
    }

    /// Sets a chain flag.
    pub fn set_flag(&mut self, name: impl Into<String>, value: bool) {
        self.chain.flags.set_flag(name, value);
    }

    /// Evaluates a condition with the chain context.
    #[must_use]
    pub fn eval(&self, condition: &dyn Condition) -> bool {
        condition.evaluate(&QueryCtx {
            target: self.chain.instigator.as_deref(),
            chain: Some(&self.chain.flags),
            caps: self.services,
        })
    }

    /// Applies effects with the chain context.
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

    use alloc::boxed::Box;

    use alloc::string::String;

    use super::*;

    const HERE: (SequenceRef, usize) = (SequenceRef::from_raw(0), 0);

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
        let mut events = Events::default();
        let ctx = Context::new(&mut services, &mut state, &mut events, HERE);
        assert_eq!(ctx.instigator_as::<i32>(), Some(&42));
        assert!(ctx.instigator_as::<String>().is_none());
    }

    #[test]
    fn context_exposes_typed_service_helpers() {
        let mut services = TypeMap::new();
        services.insert(7_u32);
        let mut state = ChainState::default();
        let mut events = Events::default();
        let mut ctx = Context::new(&mut services, &mut state, &mut events, HERE);
        assert_eq!(ctx.service::<u32>(), Some(&7));
        *ctx.service_mut::<u32>().unwrap() = 8;
        assert_eq!(ctx.service::<u32>(), Some(&8));
    }

    #[test]
    fn query_and_effect_contexts_expose_targets_and_services() {
        let target = 42_i32;
        let mut caps = TypeMap::new();
        caps.insert(7_u32);

        let query = QueryCtx {
            target: Some(&target),
            chain: None,
            caps: &caps,
        };
        assert_eq!(query.target_as::<i32>(), Some(&42));
        assert_eq!(query.service::<u32>(), Some(&7));

        let mut effect = EffectCtx {
            target: Some(&target),
            chain: None,
            caps: &mut caps,
        };
        assert_eq!(effect.target_as::<i32>(), Some(&42));
        assert_eq!(effect.service::<u32>(), Some(&7));
        *effect.service_mut::<u32>().unwrap() = 8;
        assert_eq!(effect.service::<u32>(), Some(&8));
    }
}
