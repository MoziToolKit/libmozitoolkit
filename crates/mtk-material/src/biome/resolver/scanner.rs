use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::super::hardcoded::classify_tint_category;
use super::types::BiomeResolver;

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

    /// Analyze loaded models to discover overlay pairs, tint indexes, and custom foliage/grass textures.
    pub fn analyze_models(&mut self) {
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

            let model_stem = model_name
                .strip_prefix("block/")
                .unwrap_or(&model_name)
                .split('/')
                .next_back()
                .unwrap_or(&model_name)
                .trim_end_matches(".json");

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
                                            let cat = classify_tint_category(
                                                &clean_tex,
                                                Some(model_stem),
                                                Some(ti),
                                            );
                                            if cat != "none" {
                                                self.texture_tint_categories
                                                    .insert(clean_tex, cat.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let cat = classify_tint_category("", Some(model_stem), Some(0));
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
                        let stem = Path::new(&file)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        self.models.insert(format!("block/{}", stem), val.clone());
                        self.models.insert(stem.to_string(), val);
                    }
                }
            }
        }
        self.update_models_index();
        self.analyze_models();
        Ok(())
    }

    /// Parse block models across all layers of a `ResourcePackStack`.
    pub fn load_from_pack_stack(
        &mut self,
        stack: &mtk_resource::ResourcePackStack,
    ) -> Result<(), mtk_resource::ResourceError> {
        for pack in stack.packs() {
            self.load_from_pack(pack.as_ref())?;
        }
        Ok(())
    }
}
