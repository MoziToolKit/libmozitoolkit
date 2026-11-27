use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::super::hardcoded::{classify_tint_category, get_hardcoded_tint_hex};
use super::super::palettes::hex_to_linear_rgba;
use super::types::BiomeResolver;

fn normalize_model_stem(raw: &str) -> String {
    let clean = raw.trim().to_ascii_lowercase();
    let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    unname
        .split('/')
        .next_back()
        .unwrap_or(unname)
        .trim_end_matches(".json")
        .to_string()
}

/// Recursively walks a blockstate JSON (`variants` / `multipart`) and records
/// `model_stem -> owning block_stem` mappings so custom variant models inherit
/// their parent block's authoritative tint category.
fn collect_blockstate_model_refs(
    val: &Value,
    block_stem: &str,
    out: &mut HashMap<String, Vec<String>>,
) {
    match val {
        Value::Object(map) => {
            if let Some(model_str) = map.get("model").and_then(|v| v.as_str()) {
                let model_stem = normalize_model_stem(model_str);
                if !model_stem.is_empty() {
                    let owners = out.entry(model_stem).or_default();
                    if !owners.iter().any(|b| b == block_stem) {
                        owners.push(block_stem.to_string());
                    }
                }
            }
            for child in map.values() {
                collect_blockstate_model_refs(child, block_stem, out);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_blockstate_model_refs(item, block_stem, out);
            }
        }
        _ => {}
    }
}

impl BiomeResolver {
    /// Set or update loaded block models and re-analyze.
    pub fn set_models(&mut self, models: HashMap<String, Value>) {
        self.models = models;
        self.update_models_index();
        self.overlay_pairs.clear();
        self.texture_tint_categories.clear();
        self.texture_hardcoded_colors.clear();
        self.seed_defaults();
        self.analyze_models();
    }

    /// Rebuild stem index lookup for loaded models.
    pub fn update_models_index(&mut self) {
        self.models_by_stem.clear();
        for (k, v) in &self.models {
            let stem = k
                .split('/')
                .next_back()
                .unwrap_or(k)
                .trim_end_matches(".json");
            self.models_by_stem.insert(stem.to_string(), v.clone());
        }
    }

    /// Recursively resolve parent model inheritance and merge textures/elements.
    pub fn resolve_model(&self, model_key: &str, depth: usize) -> Option<Value> {
        if depth > 10 {
            return None;
        }
        let clean = model_key.trim().to_ascii_lowercase();
        let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
        let stem = unname
            .split('/')
            .next_back()
            .unwrap_or(unname)
            .trim_end_matches(".json");

        let model_val = self
            .models
            .get(unname)
            .or_else(|| self.models.get(stem))
            .or_else(|| self.models_by_stem.get(stem))?;

        let mut resolved = model_val.clone();
        if let Some(parent_str) = resolved.get("parent").and_then(|p| p.as_str()) {
            let p_clean = parent_str.trim().to_ascii_lowercase();
            let p_unname = p_clean.strip_prefix("minecraft:").unwrap_or(&p_clean);
            if let Some(parent_resolved) = self.resolve_model(p_unname, depth + 1) {
                // Merge textures: child textures override parent
                let mut merged_textures = serde_json::Map::new();
                if let Some(p_tex) = parent_resolved.get("textures").and_then(|t| t.as_object()) {
                    for (k, v) in p_tex {
                        merged_textures.insert(k.clone(), v.clone());
                    }
                }
                if let Some(c_tex) = resolved.get("textures").and_then(|t| t.as_object()) {
                    for (k, v) in c_tex {
                        merged_textures.insert(k.clone(), v.clone());
                    }
                }
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("textures".to_string(), Value::Object(merged_textures));
                    // Inherit elements if child does not explicitly define its own
                    if !obj.contains_key("elements") {
                        if let Some(p_elem) = parent_resolved.get("elements") {
                            obj.insert("elements".to_string(), p_elem.clone());
                        }
                    }
                }
            }
        }
        Some(resolved)
    }

    /// Follow #variable reference chains in a model's textures dictionary.
    pub fn resolve_texture_var<'a>(
        &self,
        raw: &'a str,
        textures: &'a HashMap<String, String>,
    ) -> String {
        let mut curr = raw;
        for _ in 0..8 {
            if let Some(var_name) = curr.strip_prefix('#') {
                if let Some(target) = textures.get(var_name) {
                    curr = target.as_str();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let clean = curr.strip_prefix("minecraft:").unwrap_or(curr);
        clean.strip_prefix("block/").unwrap_or(clean).to_string()
    }

    /// Collect parent model stems along the inheritance chain (child's parent first).
    fn collect_parent_stems(&self, model_key: &str) -> Vec<String> {
        let mut parents = Vec::new();
        let mut curr = model_key.to_string();
        for _ in 0..10 {
            let clean = curr.trim().to_ascii_lowercase();
            let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
            let stem = normalize_model_stem(unname);
            let Some(model_val) = self
                .models
                .get(unname)
                .or_else(|| self.models.get(&stem))
                .or_else(|| self.models_by_stem.get(&stem))
            else {
                break;
            };
            let Some(parent_str) = model_val.get("parent").and_then(|p| p.as_str()) else {
                break;
            };
            let parent_stem = normalize_model_stem(parent_str);
            if parent_stem.is_empty() || parents.iter().any(|p| p == &parent_stem) {
                break;
            }
            parents.push(parent_stem);
            curr = parent_str.to_string();
        }
        parents
    }

    /// Analyze loaded models to discover overlay pairs, tint indexes, and custom foliage/grass textures.
    pub fn analyze_models(&mut self) {
        self.analyze_models_with_owners(&HashMap::new());
    }

    fn analyze_models_with_owners(&mut self, model_to_blocks: &HashMap<String, Vec<String>>) {
        let model_keys: Vec<String> = self.models.keys().cloned().collect();
        for model_name in model_keys {
            let resolved = match self.resolve_model(&model_name, 0) {
                Some(r) => r,
                None => continue,
            };

            let mut textures_map = HashMap::new();
            if let Some(textures_obj) = resolved.get("textures").and_then(|t| t.as_object()) {
                for (k, v) in textures_obj {
                    if let Some(s) = v.as_str() {
                        textures_map.insert(k.clone(), s.to_string());
                    }
                }
            }

            // Discover overlay pairs
            if let (Some(side), Some(overlay)) =
                (textures_map.get("side"), textures_map.get("overlay"))
            {
                let clean_side = self.resolve_texture_var(side, &textures_map);
                let clean_overlay = self.resolve_texture_var(overlay, &textures_map);
                if !clean_side.is_empty()
                    && !clean_overlay.is_empty()
                    && clean_side != clean_overlay
                    && !clean_side.starts_with('#')
                    && !clean_overlay.starts_with('#')
                {
                    self.overlay_pairs.insert(clean_side, clean_overlay);
                }
            }
            if let (Some(cross), Some(cross_overlay)) =
                (textures_map.get("cross"), textures_map.get("cross_overlay"))
            {
                let clean_cross = self.resolve_texture_var(cross, &textures_map);
                let clean_overlay = self.resolve_texture_var(cross_overlay, &textures_map);
                if !clean_cross.is_empty()
                    && !clean_overlay.is_empty()
                    && clean_cross != clean_overlay
                    && !clean_cross.starts_with('#')
                    && !clean_overlay.starts_with('#')
                {
                    self.overlay_pairs.insert(clean_cross, clean_overlay);
                }
            }

            let model_stem = normalize_model_stem(&model_name);

            // Candidate block names: the model's own stem, any blockstate that references
            // this variant model (e.g. `grass_block.json` -> `grass_block_0`), and its
            // parent model chain (e.g. `grass_block_0` -> `parent: block/grass_block`).
            let mut candidate_blocks: Vec<String> = vec![model_stem.clone()];
            if let Some(owners) = model_to_blocks.get(&model_stem) {
                for owner in owners {
                    if !candidate_blocks.iter().any(|b| b == owner) {
                        candidate_blocks.push(owner.clone());
                    }
                }
            }
            for parent_stem in self.collect_parent_stems(&model_name) {
                if !candidate_blocks.iter().any(|b| b == &parent_stem) {
                    candidate_blocks.push(parent_stem);
                }
            }

            let classify_with_candidates = |tex: &str, ti: i32| -> Option<(&'static str, String)> {
                for cand in &candidate_blocks {
                    let cat = classify_tint_category(tex, Some(cand), Some(ti));
                    if cat != "none" {
                        return Some((cat, cand.clone()));
                    }
                }
                None
            };

            if let Some(elements) = resolved.get("elements").and_then(|e| e.as_array()) {
                for elem in elements {
                    if let Some(faces) = elem.get("faces").and_then(|f| f.as_object()) {
                        for (_f_name, f_data) in faces {
                            let tint_idx = f_data
                                .get("tintindex")
                                .and_then(|ti| ti.as_i64())
                                .map(|ti| ti as i32);
                            if let Some(ti) = tint_idx {
                                if ti >= 0 {
                                    if let Some(raw_tex) =
                                        f_data.get("texture").and_then(|t| t.as_str())
                                    {
                                        let clean_tex =
                                            self.resolve_texture_var(raw_tex, &textures_map);
                                        if !clean_tex.is_empty() && !clean_tex.starts_with('#') {
                                            if let Some((cat, matched_block)) =
                                                classify_with_candidates(&clean_tex, ti)
                                            {
                                                self.texture_tint_categories
                                                    .insert(clean_tex.clone(), cat.to_string());
                                                if cat == "hardcoded" {
                                                    if let Some(hex) = get_hardcoded_tint_hex(
                                                        &clean_tex,
                                                    )
                                                    .or_else(|| {
                                                        get_hardcoded_tint_hex(&matched_block)
                                                    }) {
                                                        self.texture_hardcoded_colors.insert(
                                                            clean_tex,
                                                            hex_to_linear_rgba(hex),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if let Some((cat, _)) = classify_with_candidates("", 0) {
                if cat == "foliage" {
                    for tex in textures_map.values() {
                        let clean_tex = self.resolve_texture_var(tex, &textures_map);
                        if !clean_tex.is_empty() && !clean_tex.starts_with('#') {
                            self.texture_tint_categories
                                .insert(clean_tex, cat.to_string());
                        }
                    }
                }
            }
        }

        // The raw JSON trees are only needed while deriving overlay/tint maps.
        // Keeping them would serialize the entire resource-pack model set into
        // every `.mtkcache`, so release them once analysis is complete.
        self.models.clear();
        self.models_by_stem.clear();
    }

    /// Parse block models from any `ResourcePack` implementation.
    ///
    /// A model that exists but cannot be read is an error: skipping it would silently drop tint
    /// metadata and bake the block with the wrong biome colouring.
    pub fn load_from_pack(
        &mut self,
        pack: &dyn mtk_resource::ResourcePack,
    ) -> Result<(), mtk_resource::ResourceError> {
        let files = pack.list_files("assets/minecraft/models/block/");
        for file in files {
            if file.ends_with(".json") {
                if let Some(bytes) = pack.open(&file)? {
                    if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
                        self.insert_block_model(&file, val);
                    }
                }
            }
        }
        let mut model_to_blocks: HashMap<String, Vec<String>> = HashMap::new();
        for file in pack.list_files("assets/minecraft/blockstates/") {
            if file.ends_with(".json") {
                if let Some(bytes) = pack.open(&file)? {
                    if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
                        let block_stem = normalize_model_stem(&file);
                        collect_blockstate_model_refs(&val, &block_stem, &mut model_to_blocks);
                    }
                }
            }
        }
        self.update_models_index();
        self.analyze_models_with_owners(&model_to_blocks);
        Ok(())
    }

    /// Parse block models across all layers of a `ResourcePackStack`.
    ///
    /// One merged listing keeps pack priority (higher packs win) and honours each pack's
    /// `pack.mcmeta` filter; iterating the packs individually would let a lower pack's model
    /// overwrite a higher one's.
    pub fn load_from_pack_stack(
        &mut self,
        stack: &mtk_resource::ResourcePackStack,
    ) -> Result<(), mtk_resource::ResourceError> {
        for file in stack.list_asset_files("assets/minecraft/models/block/") {
            if !file.ends_with(".json") {
                continue;
            }
            let Some(bytes) = stack.open_asset_raw(&file)? else {
                continue;
            };
            if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
                self.insert_block_model(&file, val);
            }
        }
        let mut model_to_blocks: HashMap<String, Vec<String>> = HashMap::new();
        for file in stack.list_asset_files("assets/minecraft/blockstates/") {
            if !file.ends_with(".json") {
                continue;
            }
            let Some(bytes) = stack.open_asset_raw(&file)? else {
                continue;
            };
            if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
                let block_stem = normalize_model_stem(&file);
                collect_blockstate_model_refs(&val, &block_stem, &mut model_to_blocks);
            }
        }
        self.update_models_index();
        self.analyze_models_with_owners(&model_to_blocks);
        Ok(())
    }

    /// Records one block model discovered through the stack.
    fn insert_block_model(&mut self, file: &str, val: Value) {
        let stem = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        self.models.insert(format!("block/{stem}"), val.clone());
        self.models.insert(stem.to_string(), val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_resource::{MemoryPack, ResourcePackStack};

    #[test]
    fn test_scanner_registers_blockstate_and_parent_variant_tints() {
        let mut pack = MemoryPack::new("dummy");
        pack.insert(
            "assets/minecraft/blockstates/grass_block.json",
            br#"{"variants":{"snowy=false":[{"model":"block/grass_block"},{"model":"block/grass_block_0"}]}}"#.to_vec(),
        );
        pack.insert(
            "assets/minecraft/models/block/grass_block.json",
            br##"{"textures":{"top":"block/grass_block_top"},"elements":[{"faces":{"up":{"texture":"#top","tintindex":0}}}]}"##.to_vec(),
        );
        pack.insert(
            "assets/minecraft/models/block/grass_block_0.json",
            br#"{"parent":"block/grass_block","textures":{"top":"block/grass_block_top_0"}}"#
                .to_vec(),
        );
        // Custom-named short grass variant (e.g. Mizunos grass_1 -> grass_1_top, grass_2 -> grass_2a)
        pack.insert(
            "assets/minecraft/blockstates/grass.json",
            br#"{"variants":{"":[{"model":"block/grass_1"},{"model":"block/grass_2"}]}}"#.to_vec(),
        );
        pack.insert(
            "assets/minecraft/models/block/grass_1.json",
            br##"{"textures":{"top":"block/grass_1_top"},"elements":[{"faces":{"up":{"texture":"#top","tintindex":0}}}]}"##.to_vec(),
        );
        pack.insert(
            "assets/minecraft/models/block/grass_2.json",
            br##"{"textures":{"0":"block/grass_2a"},"elements":[{"faces":{"up":{"texture":"#0","tintindex":0}}}]}"##.to_vec(),
        );
        // Hardcoded lily_pad variant
        pack.insert(
            "assets/minecraft/blockstates/lily_pad.json",
            br#"{"variants":{"":[{"model":"block/lily_pad_custom"}]}}"#.to_vec(),
        );
        pack.insert(
            "assets/minecraft/models/block/lily_pad_custom.json",
            br##"{"textures":{"tex":"block/lily_pad_petal"},"elements":[{"faces":{"up":{"texture":"#tex","tintindex":0}}}]}"##.to_vec(),
        );

        let mut stack = ResourcePackStack::new();
        stack.append_pack(Box::new(pack));

        let mut resolver = BiomeResolver::new();
        resolver.load_from_pack_stack(&stack).unwrap();

        assert_eq!(
            resolver
                .get_tint_info("grass_block_top_0", None, None)
                .tint_category,
            "grass"
        );
        assert_eq!(
            resolver
                .get_tint_info("grass_1_top", None, None)
                .tint_category,
            "grass"
        );
        assert_eq!(
            resolver.get_tint_info("grass_2a", None, None).tint_category,
            "grass"
        );
        let lily = resolver.get_tint_info("lily_pad_petal", None, None);
        assert_eq!(lily.tint_category, "hardcoded");
        assert!(lily.hardcoded_color.is_some());
    }
}
