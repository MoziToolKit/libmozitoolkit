//! `serde` helpers shared by the crates that write baked payloads.

use std::collections::HashMap;
use std::hash::BuildHasher;

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

/// Serializes a `HashMap` with its entries ordered by key.
///
/// `HashMap` iteration order is randomized for every map instance, so serializing one directly
/// makes two bakes of the same input produce different payloads. Sorting here keeps lookups O(1)
/// in memory, and emitting a map (rather than a sequence of pairs) keeps the wire shape identical
/// to a plain `HashMap`, so already-written packages still deserialize.
pub fn serialize_sorted_map<K, V, H, S>(
    map: &HashMap<K, V, H>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    K: Ord + Serialize,
    V: Serialize,
    H: BuildHasher,
    S: Serializer,
{
    let mut entries: Vec<(&K, &V)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut ser = serializer.serialize_map(Some(entries.len()))?;
    for (key, value) in entries {
        ser.serialize_entry(key, value)?;
    }
    ser.end()
}
