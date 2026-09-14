//! Capabilities an entity holds.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

/// The capabilities one entity holds.
///
/// The crate never interprets a key. The host chooses the type and its
/// meaning. Keys are ordered so that iteration and evaluation are
/// deterministic.
///
/// ```
/// use plotline::CapabilitySet;
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Tech {
///     Fusion,
///     Warp,
/// }
///
/// let mut caps = CapabilitySet::from([Tech::Fusion]);
/// assert!(caps.contains(&Tech::Fusion));
/// assert!(!caps.contains(&Tech::Warp));
///
/// caps.insert(Tech::Warp);
/// assert_eq!(caps.len(), 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "K: serde::Serialize",
        deserialize = "K: Ord + serde::Deserialize<'de>"
    ))
)]
pub struct CapabilitySet<K> {
    keys: BTreeSet<K>,
}

impl<K> CapabilitySet<K> {
    /// Creates an empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: BTreeSet::new(),
        }
    }

    /// Returns the number of keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns whether the set holds no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Iterates over the keys in order.
    pub fn iter(&self) -> alloc::collections::btree_set::Iter<'_, K> {
        self.keys.iter()
    }
}

impl<K: Ord> CapabilitySet<K> {
    /// Adds a key. Returns whether the set changed.
    pub fn insert(&mut self, key: K) -> bool {
        self.keys.insert(key)
    }

    /// Removes a key. Returns whether the set changed.
    pub fn remove(&mut self, key: &K) -> bool {
        self.keys.remove(key)
    }

    /// Returns whether the set holds this key.
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.keys.contains(key)
    }

    /// Removes every key.
    pub fn clear(&mut self) {
        self.keys.clear();
    }

    /// Returns the keys of `other` that this set does not hold.
    #[must_use]
    pub fn missing_from(&self, other: &Self) -> Vec<K>
    where
        K: Clone,
    {
        other
            .iter()
            .filter(|key| !self.contains(key))
            .cloned()
            .collect()
    }
}

impl<K: Ord> Extend<K> for CapabilitySet<K> {
    fn extend<T: IntoIterator<Item = K>>(&mut self, iter: T) {
        self.keys.extend(iter);
    }
}

impl<K: Ord> FromIterator<K> for CapabilitySet<K> {
    fn from_iter<T: IntoIterator<Item = K>>(iter: T) -> Self {
        Self {
            keys: iter.into_iter().collect(),
        }
    }
}

impl<K: Ord, const N: usize> From<[K; N]> for CapabilitySet<K> {
    fn from(keys: [K; N]) -> Self {
        keys.into_iter().collect()
    }
}

impl<'a, K> IntoIterator for &'a CapabilitySet<K> {
    type Item = &'a K;
    type IntoIter = alloc::collections::btree_set::Iter<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K> IntoIterator for CapabilitySet<K> {
    type Item = K;
    type IntoIter = alloc::collections::btree_set::IntoIter<K>;

    fn into_iter(self) -> Self::IntoIter {
        self.keys.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Tech {
        Fusion,
        Warp,
        Cloaking,
    }

    #[test]
    fn a_new_set_is_empty() {
        let caps = CapabilitySet::<Tech>::new();
        assert!(caps.is_empty());
        assert_eq!(caps.len(), 0);
        assert!(!caps.contains(&Tech::Fusion));
    }

    #[test]
    fn insert_and_remove_report_change() {
        let mut caps = CapabilitySet::new();
        assert!(caps.insert(Tech::Fusion));
        assert!(
            !caps.insert(Tech::Fusion),
            "a repeat insert changes nothing"
        );
        assert!(caps.remove(&Tech::Fusion));
        assert!(!caps.remove(&Tech::Fusion));
    }

    #[test]
    fn iteration_is_ordered() {
        let caps = CapabilitySet::from([Tech::Cloaking, Tech::Fusion, Tech::Warp]);
        let seen: Vec<_> = caps.iter().copied().collect();
        assert_eq!(seen, vec![Tech::Fusion, Tech::Warp, Tech::Cloaking]);
    }

    #[test]
    fn extend_merges_sources() {
        let mut caps = CapabilitySet::from([Tech::Fusion]);
        caps.extend([Tech::Warp, Tech::Fusion]);
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn missing_from_reports_the_shortfall() {
        let held = CapabilitySet::from([Tech::Fusion]);
        let wanted = CapabilitySet::from([Tech::Fusion, Tech::Warp]);
        assert_eq!(held.missing_from(&wanted), vec![Tech::Warp]);
        assert!(wanted.missing_from(&held).is_empty());
    }

    #[test]
    fn string_keys_work() {
        let mut caps = CapabilitySet::new();
        caps.insert(alloc::string::String::from("survive-frozen"));
        assert!(caps.contains(&alloc::string::String::from("survive-frozen")));
    }

    #[test]
    fn clear_empties_the_set() {
        let mut caps = CapabilitySet::from([Tech::Fusion, Tech::Warp]);
        caps.clear();
        assert!(caps.is_empty());
    }
}
