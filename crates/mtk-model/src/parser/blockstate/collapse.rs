use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::definition::VariantEntry;
use super::state::BlockState;

/// Properties that alter semantics downstream (e.g. emissive lighting in `is_block_emissive`)
/// even when both values point to the same model JSON.
const PRESERVED_PROPS: &[&str] = &["lit"];

#[inline]
fn variant_entries_equal(a: &VariantEntry, b: &VariantEntry) -> bool {
    let a_slice = match a {
        VariantEntry::Single(m) => std::slice::from_ref(m),
        VariantEntry::List(v) => v.as_slice(),
    };
    let b_slice = match b {
        VariantEntry::Single(m) => std::slice::from_ref(m),
        VariantEntry::List(v) => v.as_slice(),
    };
    a_slice == b_slice
}

/// Folds properties in a `variants` map that have >= 2 values and never alter the resolved model
/// entry (e.g. datagen-expanded `waterlogged`, `turn`, or `visible` on Create / Steam 'n' Rails).
///
/// Returns `None` when the map has `<= 1` entry or contains unparseable keys so the caller uses
/// the raw uncollapsed enumeration path.
pub(super) fn collapse_invariant_variant_states(
    base_id: &str,
    variants: &HashMap<String, VariantEntry>,
) -> Option<Vec<String>> {
    if variants.len() <= 1 {
        return None;
    }

    let mut entries: Vec<(BTreeMap<&str, &str>, &VariantEntry)> =
        Vec::with_capacity(variants.len());
    let mut candidate_props: BTreeSet<&str> = BTreeSet::new();

    for (key, entry) in variants {
        let mut props: BTreeMap<&str, &str> = BTreeMap::new();
        if !key.trim().is_empty() {
            for pair in key.split(',') {
                let token = pair.trim();
                if token.is_empty() {
                    continue;
                }
                let (k, v) = token.split_once('=')?;
                let k = k.trim();
                let v = v.trim();
                if k.is_empty() {
                    return None;
                }
                props.insert(k, v);
                if !PRESERVED_PROPS.contains(&k) {
                    candidate_props.insert(k);
                }
            }
        }
        entries.push((props, entry));
    }

    for prop in candidate_props {
        let mut distinct_values: HashSet<&str> = HashSet::new();
        for (props, _) in &entries {
            if let Some(&val) = props.get(prop) {
                distinct_values.insert(val);
            }
        }
        if distinct_values.len() < 2 {
            continue;
        }

        let mut groups: HashMap<BTreeMap<&str, &str>, &VariantEntry> =
            HashMap::with_capacity(entries.len());
        let mut invariant = true;
        for (props, entry) in &entries {
            let mut rest = props.clone();
            rest.remove(prop);
            if let Some(&first_entry) = groups.get(&rest) {
                if !variant_entries_equal(entry, first_entry) {
                    invariant = false;
                    break;
                }
            } else {
                groups.insert(rest, *entry);
            }
        }

        if invariant {
            let mut next_entries = Vec::with_capacity(groups.len());
            let mut seen_props: HashSet<BTreeMap<&str, &str>> =
                HashSet::with_capacity(groups.len());
            for (mut props, entry) in entries {
                props.remove(prop);
                if seen_props.insert(props.clone()) {
                    next_entries.push((props, entry));
                }
            }
            entries = next_entries;
        }
    }

    let mut seen_out = BTreeSet::new();
    for (props, _) in entries {
        let state_str = if props.is_empty() {
            base_id.to_string()
        } else {
            let kv = props
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{base_id}[{kv}]")
        };
        let canon = BlockState::parse(&state_str)
            .map(|bs| bs.to_canonical_string())
            .unwrap_or(state_str);
        seen_out.insert(canon);
    }

    Some(seen_out.into_iter().collect())
}
