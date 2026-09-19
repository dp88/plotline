//! Reads a map from a data file and rejects a repeated key.

use alloc::collections::BTreeMap;
use core::fmt::{Formatter, Result as FmtResult};
use core::marker::PhantomData;

use serde::de::{Deserialize, Deserializer, Error, MapAccess, Visitor};

/// Reads a map in which each key appears once.
///
/// An author can paste a sequence or a node and forget to rename it. A plain
/// map then keeps the second entry and drops the first without a word.
pub(crate) fn deserialize<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    deserializer.deserialize_map(UniqueMap(PhantomData))
}

struct UniqueMap<K, V>(PhantomData<(K, V)>);

impl<'de, K, V> Visitor<'de> for UniqueMap<K, V>
where
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    type Value = BTreeMap<K, V>;

    fn expecting(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("a map in which each name appears once")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut entries = BTreeMap::new();
        while let Some((key, value)) = map.next_entry()? {
            if entries.contains_key(&key) {
                return Err(M::Error::custom("a name appears twice in the map"));
            }
            entries.insert(key, value);
        }
        Ok(entries)
    }
}
