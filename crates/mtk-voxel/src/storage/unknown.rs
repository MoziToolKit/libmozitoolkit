//! # Unknown Block Detection & Diagnostics
//!
//! Provides zero-cost scanning of section palettes and point clouds to detect
//! missing/custom block identifiers that lack both model definitions and textures.

use mtk_core::Direction;
use mtk_cull::is_non_mesh_placeholder_block;
use mtk_material::MaterialResolver;
use mtk_model::BakedModelDatabase;
use mtk_resource::extract_block_name;
use mtk_texture::AtlasAddressMap;
use std::collections::{BTreeSet, HashMap};

use crate::mesher::heuristic::get_unit_cube_texture_candidates;

/// Determines if a specific blockstate string represents an unknown/missing block
/// that lacks both model definitions and valid atlas textures.
pub fn is_blockstate_unknown(
    state_str: &str,
    model_db: Option<&BakedModelDatabase>,
    atlas: Option<&AtlasAddressMap>,
    custom_aliases: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    if state_str.is_empty()
        || state_str == "minecraft:air"
        || is_non_mesh_placeholder_block(state_str)
    {
        return false;
    }

    // Fluids are handled dynamically by FluidMesher
    let clean = state_str.strip_prefix("minecraft:").unwrap_or(state_str);
    let base = clean.find('[').map_or(clean, |idx| &clean[..idx]);
    let lower = base.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "water" | "flowing_water" | "lava" | "flowing_lava"
    ) {
        return false;
    }

    // Dynamic built-in entity blocks (skulls, banners, decorated pots, paintings, copper golem statues) have built-in geometry renderers
    if mtk_model::skull::SkullKind::from_block_name(base).is_some()
        || mtk_model::banner::extract_banner_color_from_state(state_str).is_some()
        || lower == "decorated_pot"
        || lower == "painting"
        || lower == "copper_golem_statue"
        || lower.ends_with("_copper_golem_statue")
    {
        return false;
    }

    // 1. Check if model database resolves this state
    if let Some(db) = model_db {
        if db.get(state_str).is_some() || db.get(clean).is_some() || db.get(base).is_some() {
            return false;
        }
    }

    // 2. Check if atlas provides unit-cube texture candidates
    if let Some(map) = atlas {
        let clean_block = extract_block_name(state_str);
        let clean_sub = clean_block
            .strip_prefix("minecraft:")
            .unwrap_or(clean_block);
        let has_texture = Direction::ALL.iter().any(|&dir| {
            let candidates = get_unit_cube_texture_candidates(clean_sub, dir);
            candidates
                .iter()
                .any(|cand| MaterialResolver::resolve(cand, custom_aliases, map).is_some())
        });
        if has_texture {
            return false;
        }
    }

    // If both model and texture are missing, this is an unknown block
    true
}

/// Scans an iterator of unique blockstate strings and returns a sorted, deduplicated
/// list of unknown block identifiers.
pub fn scan_unknown_blockstates<'a, I>(
    states: I,
    model_db: Option<&BakedModelDatabase>,
    atlas: Option<&AtlasAddressMap>,
    custom_aliases: Option<&HashMap<String, Vec<String>>>,
) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut unknown_set = BTreeSet::new();
    for st in states {
        if is_blockstate_unknown(st, model_db, atlas, custom_aliases) {
            let clean = extract_block_name(st);
            unknown_set.insert(clean.to_string());
        }
    }
    unknown_set.into_iter().collect()
}
