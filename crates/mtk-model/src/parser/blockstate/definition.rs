use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::state::BlockState;

/// A matched model variant with required transforms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantMatch {
    /// Target model JSON path, e.g. "minecraft:block/oak_stairs".
    pub model_id: String,
    /// X rotation angle in degrees: 0, 90, 180, 270.
    pub rot_x: f32,
    /// Y rotation angle in degrees: 0, 90, 180, 270.
    pub rot_y: f32,
    /// Whether UVLock is enabled for this variant.
    pub uvlock: bool,
    /// Variant selection weight.
    pub weight: u32,
    /// Additional variant properties if applicable.
    pub variant_props: Option<BTreeMap<String, String>>,
}

impl Default for VariantMatch {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            rot_x: 0.0,
            rot_y: 0.0,
            uvlock: false,
            weight: 1,
            variant_props: None,
        }
    }
}

/// Single model application entry in a variant or multipart definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantModel {
    /// Model identifier path.
    pub model: String,
    /// X rotation in degrees (0, 90, 180, 270).
    #[serde(default)]
    pub x: f32,
    /// Y rotation in degrees (0, 90, 180, 270).
    #[serde(default)]
    pub y: f32,
    /// UV lock boolean.
    #[serde(default)]
    pub uvlock: bool,
    /// Selection weight (defaults to 1).
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

/// Variant entry which can be a single model or a list of weighted models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VariantEntry {
    Single(VariantModel),
    List(Vec<VariantModel>),
}

impl VariantEntry {
    /// Chooses the primary model (highest weight or first entry).
    pub fn select_primary(&self) -> &VariantModel {
        match self {
            VariantEntry::Single(m) => m,
            VariantEntry::List(list) => {
                let mut best = &list[0];
                for item in list.iter().skip(1) {
                    if item.weight > best.weight {
                        best = item;
                    }
                }
                best
            }
        }
    }
}

/// Condition specification in a multipart rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MultipartCondition {
    /// OR condition: any item in the list matching satisfies the rule.
    Or {
        #[serde(alias = "or", rename = "OR")]
        or: Vec<MultipartCondition>,
    },
    /// AND condition: all items in the list matching satisfies the rule.
    And {
        #[serde(alias = "and", rename = "AND")]
        and: Vec<MultipartCondition>,
    },
    /// Flat dictionary: ALL keys must match (AND condition).
    /// Values can be boolean, integer, or pipe-separated multiple choices (e.g. "north|south").
    Properties(HashMap<String, serde_json::Value>),
}

/// Multipart rule definition containing an optional condition and model to apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultipartRule {
    /// Activation condition. If None, always applied.
    #[serde(default)]
    pub when: Option<MultipartCondition>,
    /// Model variant to apply.
    pub apply: VariantEntry,
}

/// Representation of a Minecraft BlockState definition JSON (assets/<namespace>/blockstates/<name>.json).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockStateDefinition {
    /// Standard key-value variants map.
    #[serde(default)]
    pub variants: Option<HashMap<String, VariantEntry>>,
    /// Multipart rules array for composite blocks (fences, walls, redstone wires, etc.).
    #[serde(default)]
    pub multipart: Option<Vec<MultipartRule>>,
}

impl BlockStateDefinition {
    /// Enumerates all canonical blockstate strings described by this definition.
    ///
    /// # Arguments
    /// - `base_id`: Canonical block id, e.g. `"minecraft:oak_stairs"` or `"minecraft:stone"`
    pub fn enumerate_all_states(&self, base_id: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut seen = BTreeSet::new();

        if let Some(ref variants) = self.variants {
            if !variants.is_empty() {
                if variants.len() == 1 && variants.contains_key("") {
                    if let Some(expanded) = Self::expand_known_entity_states(base_id) {
                        for st in expanded {
                            if let Ok(bs) = BlockState::parse(&st) {
                                let canon = bs.to_canonical_string();
                                if seen.insert(canon.clone()) {
                                    results.push(canon);
                                }
                            }
                        }
                        if seen.insert(base_id.to_string()) {
                            results.push(base_id.to_string());
                        }
                        return results;
                    }
                }

                for key in variants.keys() {
                    let state_str = if key.is_empty() {
                        base_id.to_string()
                    } else {
                        format!("{}[{}]", base_id, key)
                    };
                    if let Ok(bs) = BlockState::parse(&state_str) {
                        let canon = bs.to_canonical_string();
                        if seen.insert(canon.clone()) {
                            results.push(canon);
                        }
                    } else if seen.insert(state_str.clone()) {
                        results.push(state_str);
                    }
                }
                return results;
            }
        }

        if let Some(ref multipart) = self.multipart {
            if !multipart.is_empty() {
                let mut prop_values: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

                for rule in multipart {
                    if let Some(ref when) = rule.when {
                        Self::extract_condition_prop_values(when, &mut prop_values);
                    }
                }

                // If boolean properties only had true or false, expand to both
                for (k, vals) in prop_values.iter_mut() {
                    if vals.contains("true") || vals.contains("false") {
                        vals.insert("true".to_string());
                        vals.insert("false".to_string());
                    }
                    if k == "flower_amount" || k == "flowers" || k == "pickles" || k == "candles" {
                        vals.insert("1".to_string());
                    }
                    // Walls / Pale Moss: Direction connection properties containing "low" or "tall" also have "none" in Minecraft
                    if vals.contains("low") || vals.contains("tall") {
                        vals.insert("none".to_string());
                    }
                    // Bamboo: leaves can be "none"
                    if k == "leaves" && (vals.contains("small") || vals.contains("large")) {
                        vals.insert("none".to_string());
                    }
                    // Composter: level can be "0"
                    if k == "level" && vals.contains("1") {
                        vals.insert("0".to_string());
                    }
                }

                // Never expand non-geometric properties (waterlogged, distance, persistent, etc.)
                const NON_GEOMETRIC_PROPS: &[&str] = &[
                    "waterlogged",
                    "distance",
                    "persistent",
                    "occupied",
                    "conditional",
                    "stage",
                    "unstable",
                    "disarmed",
                ];
                for &p in NON_GEOMETRIC_PROPS {
                    prop_values.remove(p);
                }

                if prop_values.is_empty() {
                    results.push(base_id.to_string());
                } else {
                    let keys: Vec<String> = prop_values.keys().cloned().collect();
                    let value_lists: Vec<Vec<String>> = keys
                        .iter()
                        .map(|k| prop_values[k].iter().cloned().collect())
                        .collect();

                    let mut total_combos = 1usize;
                    for list in &value_lists {
                        total_combos = total_combos.saturating_mul(list.len());
                    }

                    if total_combos <= 2048 {
                        let mut combos: Vec<Vec<(String, String)>> = vec![Vec::new()];
                        for (key, values) in keys.iter().zip(value_lists.iter()) {
                            let mut next_combos = Vec::new();
                            for existing in combos {
                                for val in values {
                                    let mut cloned = existing.clone();
                                    cloned.push((key.clone(), val.clone()));
                                    next_combos.push(cloned);
                                }
                            }
                            combos = next_combos;
                        }

                        for combo in combos {
                            let props_str: Vec<String> = combo
                                .into_iter()
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect();
                            let state_str = format!("{}[{}]", base_id, props_str.join(","));
                            if let Ok(bs) = BlockState::parse(&state_str) {
                                let canon = bs.to_canonical_string();
                                if seen.insert(canon.clone()) {
                                    results.push(canon);
                                }
                            } else if seen.insert(state_str.clone()) {
                                results.push(state_str);
                            }
                        }

                        // Also add base unparameterized block ID fallback
                        if seen.insert(base_id.to_string()) {
                            results.push(base_id.to_string());
                        }
                    } else {
                        // Fallback to base id if combinatorial explosion
                        results.push(base_id.to_string());
                    }
                }
                return results;
            }
        }

        if seen.insert(base_id.to_string()) {
            results.push(base_id.to_string());
        }
        results
    }

    pub fn extract_condition_prop_values(
        condition: &MultipartCondition,
        prop_values: &mut BTreeMap<String, BTreeSet<String>>,
    ) {
        match condition {
            MultipartCondition::Or { or } => {
                for sub in or {
                    Self::extract_condition_prop_values(sub, prop_values);
                }
            }
            MultipartCondition::And { and } => {
                for sub in and {
                    Self::extract_condition_prop_values(sub, prop_values);
                }
            }
            MultipartCondition::Properties(map) => {
                for (k, val) in map {
                    let set = prop_values.entry(k.clone()).or_default();
                    match val {
                        serde_json::Value::Bool(b) => {
                            set.insert(b.to_string());
                        }
                        serde_json::Value::Number(n) => {
                            set.insert(n.to_string());
                        }
                        serde_json::Value::String(s) => {
                            for item in s.split('|') {
                                set.insert(item.trim().to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn expand_known_entity_states(base_id: &str) -> Option<Vec<String>> {
        if let Some(builtin) =
            crate::builtin::BuiltinModelRegistry::get_builtin_blockstate_def(base_id)
        {
            if let Some(ref variants) = builtin.variants {
                if !variants.is_empty() {
                    let mut list = Vec::new();
                    for key in variants.keys() {
                        if key.is_empty() {
                            list.push(base_id.to_string());
                        } else {
                            list.push(format!("{}[{}]", base_id, key));
                        }
                    }
                    return Some(list);
                }
            }
        }

        let short_name = base_id.strip_prefix("minecraft:").unwrap_or(base_id);

        if short_name == "chest" || short_name == "trapped_chest" || short_name.ends_with("_chest")
        {
            if short_name == "chest_boat" || short_name.ends_with("_chest_boat") {
                return None;
            }
            let mut list = Vec::new();
            let facings = ["north", "south", "east", "west"];
            if short_name == "ender_chest" {
                for f in facings {
                    list.push(format!("{}[facing={}]", base_id, f));
                }
            } else {
                let types = ["single", "left", "right"];
                for f in facings {
                    for t in types {
                        list.push(format!("{}[facing={},type={}]", base_id, f, t));
                    }
                }
            }
            return Some(list);
        }

        if short_name.ends_with("_head") || short_name.ends_with("_skull") {
            let mut list = Vec::new();
            if short_name.contains("_wall_") {
                for f in ["north", "south", "east", "west"] {
                    list.push(format!("{}[facing={}]", base_id, f));
                }
            } else {
                for r in 0..16 {
                    list.push(format!("{}[rotation={}]", base_id, r));
                }
            }
            return Some(list);
        }

        if short_name == "shulker_box" || short_name.ends_with("_shulker_box") {
            let mut list = Vec::new();
            for f in ["down", "up", "north", "south", "west", "east"] {
                list.push(format!("{}[facing={}]", base_id, f));
            }
            return Some(list);
        }

        if short_name.ends_with("_banner") {
            let mut list = Vec::new();
            if short_name.contains("_wall_") {
                for f in ["north", "south", "east", "west"] {
                    list.push(format!("{}[facing={}]", base_id, f));
                }
            } else {
                for r in 0..16 {
                    list.push(format!("{}[rotation={}]", base_id, r));
                }
            }
            return Some(list);
        }

        if short_name.contains("hanging_sign") {
            let mut list = Vec::new();
            if short_name.contains("_wall_") {
                for f in ["north", "south", "east", "west"] {
                    list.push(format!("{}[facing={}]", base_id, f));
                }
            } else {
                for attached in ["false", "true"] {
                    for r in 0..16 {
                        list.push(format!("{}[attached={},rotation={}]", base_id, attached, r));
                    }
                }
            }
            return Some(list);
        }

        if short_name.ends_with("_sign") {
            let mut list = Vec::new();
            if short_name.contains("_wall_") {
                for f in ["north", "south", "east", "west"] {
                    list.push(format!("{}[facing={}]", base_id, f));
                }
            } else {
                for r in 0..16 {
                    list.push(format!("{}[rotation={}]", base_id, r));
                }
            }
            return Some(list);
        }

        if short_name.contains("copper_golem_statue") {
            let mut list = Vec::new();
            for pose in ["standing", "sitting", "running", "star"] {
                for facing in ["north", "south", "east", "west"] {
                    list.push(format!(
                        "{}[copper_golem_pose={},facing={}]",
                        base_id, pose, facing
                    ));
                }
            }
            return Some(list);
        }

        if short_name == "decorated_pot" {
            let mut list = Vec::new();
            for f in ["north", "south", "east", "west"] {
                list.push(format!("{}[facing={}]", base_id, f));
            }
            return Some(list);
        }

        None
    }
}
