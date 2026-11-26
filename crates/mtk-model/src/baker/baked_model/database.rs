use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::model::BakedModel;
use super::variant_group::BakedVariantGroup;

/// In-memory database of baked models keyed by canonical BlockState strings.
#[derive(Debug, Clone, Default)]
pub struct BakedModelDatabase {
    pub models: HashMap<String, BakedModel>,
    pub variant_groups: HashMap<String, BakedVariantGroup>,
    /// Models that were discovered in the resource stack but could not be baked.
    ///
    /// Bake diagnostics describe one particular run, not the cached data, so they are
    /// intentionally excluded from [`BakedModelDatabase`]'s serialized form.
    bake_failures: Vec<ModelBakeFailure>,
}

/// One model that was discovered but produced no baked geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelBakeFailure {
    /// Canonical model identifier, e.g. `minecraft:item/shield`.
    pub key: String,
    /// Machine-readable reason, e.g. `no_geometry`, `no_texture_layers`.
    pub reason: String,
}

#[derive(Serialize)]
struct CompactDatabasePayload<'a> {
    #[serde(serialize_with = "mtk_core::serialize_sorted_map")]
    models: &'a HashMap<String, BakedModel>,
    #[serde(serialize_with = "mtk_core::serialize_sorted_map")]
    multi_variants: HashMap<String, &'a BakedVariantGroup>,
}

#[derive(Deserialize)]
struct CompactDatabaseOwnedPayload {
    models: HashMap<String, BakedModel>,
    #[serde(default)]
    variant_groups: HashMap<String, BakedVariantGroup>,
}

impl Serialize for BakedModelDatabase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Only store multi-variant groups with more than 1 variant in multi_variants.
        // Single-variant groups are identical to the primary model in `models`.
        let mut multi_variants = HashMap::new();
        for (k, v) in &self.variant_groups {
            if v.models.len() > 1 {
                multi_variants.insert(k.clone(), v);
            }
        }
        let payload = CompactDatabasePayload {
            models: &self.models,
            multi_variants,
        };
        payload.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BakedModelDatabase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let payload = CompactDatabaseOwnedPayload::deserialize(deserializer)?;
        let mut models = payload.models;
        let mut variant_groups = payload.variant_groups;

        // Ensure every state in variant_groups is present in models
        for (st, group) in &variant_groups {
            if !models.contains_key(st) {
                models.insert(st.clone(), group.select_primary().clone());
            }
        }

        // Reconstruct single-variant groups for states present in models but not in multi_variants
        for (st, model) in &models {
            variant_groups
                .entry(st.clone())
                .or_insert_with(|| BakedVariantGroup::single(st.clone(), model.clone()));
        }

        Ok(Self {
            models,
            variant_groups,
            bake_failures: Vec::new(),
        })
    }
}

impl BakedModelDatabase {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            variant_groups: HashMap::new(),
            bake_failures: Vec::new(),
        }
    }

    /// Records a model that was discovered but could not be baked into geometry.
    ///
    /// The first reason recorded for a key wins: the same model can be reached through several
    /// pipeline stages (resolution, then baking) and the earliest failure is the informative one.
    pub fn push_bake_failure(&mut self, key: impl Into<String>, reason: impl Into<String>) {
        let key = key.into();
        if self.bake_failures.iter().any(|failure| failure.key == key) {
            return;
        }
        self.bake_failures.push(ModelBakeFailure {
            key,
            reason: reason.into(),
        });
    }

    /// Models discovered in the resource stack that produced no baked geometry.
    pub fn bake_failures(&self) -> &[ModelBakeFailure] {
        &self.bake_failures
    }

    pub fn insert(&mut self, state: String, model: BakedModel) {
        self.variant_groups
            .entry(state.clone())
            .or_insert_with(|| BakedVariantGroup::single(state.clone(), model.clone()));
        self.models.insert(state, model);
    }

    /// Inserts a multi-variant group, registering its primary model into `self.models`.
    pub fn insert_variant_group(&mut self, state: String, group: BakedVariantGroup) {
        self.models
            .insert(state.clone(), group.select_primary().clone());
        self.variant_groups.insert(state, group);
    }

    /// Resolves the variant group for a BlockState if available.
    pub fn get_variant_group(&self, state: &str) -> Option<&BakedVariantGroup> {
        if let Some(group) = self.variant_groups.get(state) {
            return Some(group);
        }
        if let Ok(parsed) = crate::parser::blockstate::BlockState::parse(state) {
            let canon = parsed.to_canonical_string();
            if let Some(group) = self.variant_groups.get(&canon) {
                return Some(group);
            }
        }
        self.get(state)
            .and_then(|model| self.variant_groups.get(&model.block_state))
    }

    /// Eliminates overlapping, duplicate, and interior coplanar contacting faces
    /// across all baked models in the database, returning total faces removed.
    pub fn deduplicate_all(&mut self) -> usize {
        let mut total = 0;
        for model in self.models.values_mut() {
            total += model.deduplicate_faces();
        }
        for group in self.variant_groups.values_mut() {
            total += group.deduplicate_faces();
        }
        total
    }

    /// Remaps all baked models in the database to atlas coordinates using a lookup closure.
    pub fn remap_to_atlas_with<F>(&mut self, mut lookup_fn: F)
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        for model in self.models.values_mut() {
            model.remap_to_atlas_with(&mut lookup_fn);
        }
        for group in self.variant_groups.values_mut() {
            group.remap_to_atlas_with(&mut lookup_fn);
        }
    }

    /// Resolves a baked model using multi-tiered smart BlockState resolution:
    /// 1. Exact string match in `self.models`.
    /// 2. Normalized match with non-geometric properties stripped (e.g. `waterlogged`, `occupied`).
    /// 3. Best compatibility subset match against variant keys for this block ID.
    /// 4. Base unparameterized block ID fallback (e.g. `minecraft:chest`).
    /// 5. Canonical Mojang version-rename peer fallback when a save/world and baked pack
    ///    straddle a vanilla block ID rename (e.g. 1.21 `chain` <-> 1.21.9+ `iron_chain`).
    pub fn get(&self, state: &str) -> Option<&BakedModel> {
        // Tier 1: Fast exact match
        if let Some(model) = self.models.get(state) {
            return Some(model);
        }

        let parsed = crate::parser::blockstate::BlockState::parse(state).ok()?;
        let base_id = parsed.block_id();

        if let Some(model) = self.resolve_block_variants(&base_id, &parsed.name, &parsed.properties)
        {
            return Some(model);
        }

        if parsed.namespace == "minecraft" {
            if let Some(alias) = vanilla_renamed_block_alias(&parsed.name) {
                let alias_base = format!("minecraft:{}", alias);
                if let Some(model) =
                    self.resolve_block_variants(&alias_base, alias, &parsed.properties)
                {
                    return Some(model);
                }
            }
        }

        None
    }

    fn resolve_block_variants(
        &self,
        base_id: &str,
        short_name: &str,
        properties: &BTreeMap<String, String>,
    ) -> Option<&BakedModel> {
        let canon_str = format_canonical_blockstate(base_id, properties);

        // Tier 1.5: Canonical match (handles whitespace, property ordering, and renamed base_id)
        if let Some(model) = self.models.get(&canon_str) {
            return Some(model);
        }

        // Tier 2: Strip known non-geometric properties that never affect block model geometry in vanilla
        const NON_GEOMETRIC_PROPS: &[&str] = &[
            "power",
            "waterlogged",
            "occupied",
            "distance",
            "persistent",
            "stage",
            "unstable",
            "conditional",
            "disarmed",
        ];

        let mut has_non_geom = false;
        let mut filtered_props = properties.clone();
        for &prop in NON_GEOMETRIC_PROPS {
            if filtered_props.remove(prop).is_some() {
                has_non_geom = true;
            }
        }

        if has_non_geom {
            let canon_filtered = format_canonical_blockstate(base_id, &filtered_props);

            if let Some(model) = self.models.get(&canon_filtered) {
                return Some(model);
            }
        }

        // Helper closure to find the best compatible variant for a given base_id and property map
        let find_best_variant = |target_base_id: &str,
                                 target_props: &BTreeMap<String, String>|
         -> Option<&BakedModel> {
            let prefix = format!("{}[", target_base_id);
            let mut best_model: Option<&BakedModel> = None;
            let mut best_score = -999999i32;

            let mut relaxed_best_model: Option<&BakedModel> = None;
            let mut relaxed_best_score = -999999i32;

            for (key, model) in &self.models {
                if key == target_base_id {
                    let score = if target_props.is_empty() { 1000 } else { 0 };
                    if score > best_score {
                        best_score = score;
                        best_model = Some(model);
                    }
                    if score > relaxed_best_score {
                        relaxed_best_score = score;
                        relaxed_best_model = Some(model);
                    }
                    continue;
                }

                if key.starts_with(&prefix) && key.ends_with(']') {
                    let cand_props_str = &key[prefix.len()..key.len() - 1];
                    let mut cand_props: HashMap<&str, &str> = HashMap::new();
                    for pair in cand_props_str.split(',') {
                        if let Some((k, v)) = pair.split_once('=') {
                            cand_props.insert(k.trim(), v.trim());
                        }
                    }

                    // Compatibility check:
                    // Any property specified in target_props MUST match candidate's value if candidate defines it
                    let mut compatible = true;
                    let mut matched_keys = 0i32;
                    let mut relaxed_score = 0i32;

                    for (k, v) in target_props {
                        if NON_GEOMETRIC_PROPS.contains(&k.as_str()) {
                            continue;
                        }
                        if let Some(&cand_v) = cand_props.get(k.as_str()) {
                            if cand_v == v.as_str() {
                                matched_keys += 1;
                                relaxed_score += if k == "facing" || k == "axis" {
                                    500
                                } else {
                                    100
                                };
                            } else {
                                compatible = false;
                                if k == "facing" || k == "axis" {
                                    relaxed_score -= 300;
                                }
                            }
                        }
                    }

                    if compatible {
                        let mut score = matched_keys * 100;
                        if cand_props.len() == target_props.len() {
                            score += 1000;
                        }

                        // Score candidate properties that were NOT specified in the query:
                        // Prefer canonical vanilla default values!
                        for (cand_k, cand_v) in &cand_props {
                            if !target_props.contains_key(*cand_k) {
                                if *cand_k == "up"
                                    && (target_base_id.ends_with("_wall")
                                        || target_base_id == "wall")
                                {
                                    // In vanilla Minecraft, wall blocks default to up=true (post enabled)
                                    if *cand_v == "true" {
                                        score += 10;
                                    } else {
                                        score -= 10;
                                    }
                                } else if matches!(
                                    *cand_v,
                                    "false"
                                        | "0"
                                        | "none"
                                        | "straight"
                                        | "bottom"
                                        | "lower"
                                        | "single"
                                        | "foot"
                                        | "normal"
                                        | "y"
                                        | "north"
                                ) {
                                    score += 10;
                                } else if matches!(
                                    *cand_v,
                                    "true"
                                        | "1"
                                        | "top"
                                        | "upper"
                                        | "head"
                                        | "inner"
                                        | "outer"
                                        | "double"
                                        | "x"
                                        | "z"
                                        | "south"
                                        | "east"
                                        | "west"
                                        | "side"
                                        | "up"
                                        | "low"
                                        | "tall"
                                ) {
                                    score -= 10;
                                }
                            }
                        }

                        if score > best_score {
                            best_score = score;
                            best_model = Some(model);
                        }
                    }

                    if relaxed_score > relaxed_best_score {
                        relaxed_best_score = relaxed_score;
                        relaxed_best_model = Some(model);
                    }
                }
            }

            best_model.or(relaxed_best_model)
        };

        // Tier 3: Cross-category block mappings for directional variants
        // If torch has a horizontal facing (north/south/east/west), it is a wall torch
        let namespace = base_id.split_once(':').map_or("minecraft", |(ns, _)| ns);
        if let Some(facing) = filtered_props.get("facing").map(|s| s.as_str()) {
            if matches!(facing, "north" | "south" | "east" | "west") {
                let wall_id = match short_name {
                    "torch" => Some(format!("{}:wall_torch", namespace)),
                    "soul_torch" => Some(format!("{}:soul_wall_torch", namespace)),
                    "redstone_torch" => Some(format!("{}:redstone_wall_torch", namespace)),
                    _ => None,
                };
                if let Some(wid) = wall_id {
                    if let Some(m) = find_best_variant(&wid, &filtered_props) {
                        return Some(m);
                    }
                }
            }
        }

        // Tier 3.5: Compatibility match on base_id with filtered_props
        if let Some(m) = find_best_variant(base_id, &filtered_props) {
            return Some(m);
        }

        // Also try stripped short name (e.g. without "minecraft:" namespace)
        if base_id != short_name {
            if let Some(m) = find_best_variant(short_name, &filtered_props) {
                return Some(m);
            }
        }

        // Check wall_torch -> torch if facing is up or not found
        let standing_id = match short_name {
            "wall_torch" => Some(format!("{}:torch", namespace)),
            "soul_wall_torch" => Some(format!("{}:soul_torch", namespace)),
            "redstone_wall_torch" => Some(format!("{}:redstone_torch", namespace)),
            _ => None,
        };
        if let Some(sid) = standing_id {
            let mut standing_props = filtered_props.clone();
            standing_props.remove("facing");
            if let Some(m) = find_best_variant(&sid, &standing_props) {
                return Some(m);
            }
        }

        // Tier 5: Base block ID fallback (e.g. "minecraft:chest")
        if let Some(m) = self.models.get(base_id) {
            return Some(m);
        }

        // Also try stripped short name (e.g. "chest")
        self.models.get(short_name)
    }

    /// Resolves an item model by item identifier (e.g. "minecraft:diamond_sword", "diamond_sword", "item/diamond_sword")
    /// with multi-tiered resolution and block-state fallback for block items.
    ///
    /// A caller that names an explicit `item/...` model (for example
    /// `minecraft:item/chest`) is never answered with a fuzzy blockstate variant: the
    /// block model carries block geometry and block-context display data, so substituting
    /// it makes the item render with guessed scale and orientation. Missing item models
    /// return `None` instead.
    pub fn get_item(&self, item_id: &str) -> Option<&BakedModel> {
        let clean = item_id.trim();
        if clean.is_empty() {
            return None;
        }
        if let Some(m) = self.models.get(clean) {
            return Some(m);
        }

        let (namespace, path) = match clean.split_once(':') {
            Some((ns, p)) => (ns, p),
            None => ("minecraft", clean),
        };

        let explicit_item_model = path.starts_with("item/");
        let sub_path = path.strip_prefix("item/").unwrap_or(path);

        // An explicit `item/...` request may only be answered with item-model entries.
        // Falling back to `minecraft:<name>` would silently hand back the blockstate entry,
        // whose geometry and display data belong to block rendering.
        if explicit_item_model {
            for cand in [
                format!("{}:item/{}", namespace, sub_path),
                format!("item/{}", sub_path),
            ] {
                if let Some(m) = self.models.get(&cand) {
                    return Some(m);
                }
            }
            if namespace == "minecraft" {
                if let Some(alias) = vanilla_renamed_block_alias(sub_path) {
                    for cand in [
                        format!("minecraft:item/{}", alias),
                        format!("item/{}", alias),
                    ] {
                        if let Some(m) = self.models.get(&cand) {
                            return Some(m);
                        }
                    }
                }
            }
            return None;
        }

        let candidates = [
            format!("{}:item/{}", namespace, sub_path),
            format!("{}:{}", namespace, sub_path),
            format!("item/{}", sub_path),
            sub_path.to_string(),
        ];
        for cand in &candidates {
            if let Some(m) = self.models.get(cand) {
                return Some(m);
            }
        }

        if namespace == "minecraft" {
            if let Some(alias) = vanilla_renamed_block_alias(sub_path) {
                for cand in [
                    format!("minecraft:item/{}", alias),
                    format!("item/{}", alias),
                ] {
                    if let Some(m) = self.models.get(&cand) {
                        return Some(m);
                    }
                }
            }
        }

        // Bare block ids may resolve a representative blockstate variant (e.g.
        // `minecraft:chest` has no unparameterised state) because that is the correct
        // geometry for a block item.
        self.get(clean)
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.models.keys()
    }
}

fn format_canonical_blockstate(base_id: &str, props: &BTreeMap<String, String>) -> String {
    if props.is_empty() {
        base_id.to_string()
    } else {
        let props_str: Vec<String> = props.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        format!("{}[{}]", base_id, props_str.join(","))
    }
}

fn vanilla_renamed_block_alias(name: &str) -> Option<&'static str> {
    match name {
        // 1.21.9+ (`DataVersion >= 4541`) renamed `minecraft:chain` to `minecraft:iron_chain`.
        "iron_chain" => Some("chain"),
        "chain" => Some("iron_chain"),
        // 1.20.3 (`DataVersion >= 3692`) renamed `minecraft:grass` to `minecraft:short_grass`.
        "short_grass" => Some("grass"),
        "grass" => Some("short_grass"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baked_model_database_get_item() {
        let mut db = BakedModelDatabase::new();
        let sword = BakedModel {
            block_state: "minecraft:item/diamond_sword".to_string(),
            ..Default::default()
        };
        db.insert("minecraft:item/diamond_sword".to_string(), sword);

        let mod_item = BakedModel {
            block_state: "techmod:item/wrench".to_string(),
            ..Default::default()
        };
        db.insert("techmod:item/wrench".to_string(), mod_item);

        let stone_block = BakedModel {
            block_state: "minecraft:stone".to_string(),
            ..Default::default()
        };
        db.insert("minecraft:stone".to_string(), stone_block);

        assert!(db.get_item("minecraft:item/diamond_sword").is_some());
        assert!(db.get_item("item/diamond_sword").is_some());
        assert!(db.get_item("minecraft:diamond_sword").is_some());
        assert!(db.get_item("diamond_sword").is_some());
        assert!(db.get_item("iron_sword").is_none());

        // Mod namespace item
        assert!(db.get_item("techmod:wrench").is_some());
        assert!(db.get_item("techmod:item/wrench").is_some());

        // Block item fallback
        assert!(db.get_item("minecraft:stone").is_some());
        assert!(db.get_item("stone").is_some());
    }

    #[test]
    fn test_cross_version_vanilla_rename_fallback() {
        // 1.21 pack stores `minecraft:chain`; save loader upgrades `<4541` `chain` -> `iron_chain`.
        let mut db_121 = BakedModelDatabase::new();
        let chain_x = BakedModel {
            block_state: "minecraft:chain[axis=x]".to_string(),
            ..Default::default()
        };
        let chain_item = BakedModel {
            block_state: "minecraft:item/chain".to_string(),
            ..Default::default()
        };
        db_121.insert("minecraft:chain[axis=x]".to_string(), chain_x);
        db_121.insert("minecraft:item/chain".to_string(), chain_item);

        let resolved_block = db_121
            .get("minecraft:iron_chain[axis=x,waterlogged=false]")
            .expect("1.21 chain must satisfy upgraded iron_chain blockstate");
        assert_eq!(resolved_block.block_state, "minecraft:chain[axis=x]");

        let resolved_item = db_121
            .get_item("minecraft:item/iron_chain")
            .expect("1.21 item/chain must satisfy item/iron_chain lookup");
        assert_eq!(resolved_item.block_state, "minecraft:item/chain");

        // Primary ID always wins over alias when the newer pack ships `iron_chain`.
        let mut db_26 = BakedModelDatabase::new();
        db_26.insert(
            "minecraft:iron_chain[axis=x]".to_string(),
            BakedModel {
                block_state: "minecraft:iron_chain[axis=x]".to_string(),
                ..Default::default()
            },
        );
        db_26.insert(
            "minecraft:chain[axis=x]".to_string(),
            BakedModel {
                block_state: "minecraft:chain[axis=x]".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            db_26
                .get("minecraft:iron_chain[axis=x]")
                .expect("exact iron_chain must take priority")
                .block_state,
            "minecraft:iron_chain[axis=x]"
        );
        // Reverse direction (`chain` query against 26.2 pack having only `iron_chain`),
        // and cross-version miss when neither exists.
        assert!(db_121.get("minecraft:short_grass").is_none());
        let mut db_120 = BakedModelDatabase::new();
        db_120.insert(
            "minecraft:grass".to_string(),
            BakedModel {
                block_state: "minecraft:grass".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            db_120
                .get("minecraft:short_grass")
                .expect("1.20 grass must satisfy upgraded short_grass")
                .block_state,
            "minecraft:grass"
        );
    }
}
