use super::sprite::{is_companion_asset_path, DiscoveredSprite};
use super::ResourcePackStack;
use crate::atlas::{AtlasDefinition, AtlasSource};
use crate::error::ResourceError;
use crate::identifier::ResourceLocation;
use std::collections::HashSet;

impl ResourcePackStack {
    /// Scan all active resource packs and load all OptiFine / Continuity CTM rules.
    pub fn load_ctm_rules(&self) -> Vec<crate::ctm::CtmRule> {
        let mut rules = Vec::new();
        let mut seen_paths = HashSet::new();

        for pack in &self.packs {
            for file in pack.list_files("assets/") {
                if !file.ends_with(".properties") {
                    continue;
                }
                if !file.contains("/optifine/ctm/") && !file.contains("/textures/") {
                    continue;
                }
                if seen_paths.contains(&file) {
                    continue;
                }
                seen_paths.insert(file.clone());

                if let Some(bytes) = pack.open(&file) {
                    if let Ok(content) = std::str::from_utf8(&bytes) {
                        let parts: Vec<&str> = file.split('/').collect();
                        let namespace = if parts.len() > 1 {
                            parts[1]
                        } else {
                            crate::identifier::DEFAULT_NAMESPACE
                        };
                        let rel_path = parts[2..].join("/");
                        if let Some(rule) =
                            crate::CtmRule::parse_properties(&rel_path, namespace, content)
                        {
                            rules.push(rule);
                        }
                    }
                }
            }
        }

        rules.sort_by_key(|r| std::cmp::Reverse(r.priority));
        rules
    }

    /// Constructs an indexed `CtmSolver` containing all active CTM rules loaded from this stack.
    pub fn build_ctm_solver(&self) -> crate::ctm::CtmSolver {
        crate::ctm::CtmSolver::new(self.load_ctm_rules())
    }

    /// Collect and expand all Sprite references declared in an AtlasDefinition into discrete `DiscoveredSprite`s.
    pub fn collect_sprites_for_atlas(
        &self,
        definition: &AtlasDefinition,
    ) -> Result<Vec<DiscoveredSprite>, ResourceError> {
        let mut results = Vec::new();
        let mut registered_sprites: HashSet<ResourceLocation> = HashSet::new();

        for source in &definition.sources {
            match source {
                AtlasSource::Directory {
                    source: dir_src,
                    prefix,
                } => {
                    let mut found_paths = HashSet::new();
                    // Scan all packs in stack
                    for pack in &self.packs {
                        for file in pack.list_files("assets/") {
                            if !file.ends_with(".png") {
                                continue;
                            }
                            // Companion textures (_n, _s, _overlay) must never become standalone sprites
                            if is_companion_asset_path(&file) {
                                continue;
                            }

                            if let Some(loc) =
                                ResourceLocation::from_asset_path(&file, "textures", "png")
                            {
                                if loc.path == *dir_src
                                    || loc.path.starts_with(&format!("{}/", dir_src))
                                {
                                    found_paths.insert(loc);
                                }
                            }
                        }
                    }

                    // Convert to canonical Sprite locations with prefix
                    for tex_loc in found_paths {
                        let short = if tex_loc.path == *dir_src {
                            ""
                        } else {
                            tex_loc
                                .path
                                .strip_prefix(&format!("{}/", dir_src))
                                .unwrap_or(&tex_loc.path)
                        };
                        let sprite_path = format!("{}{}", prefix, short);
                        let sprite_id = ResourceLocation::new(&tex_loc.namespace, sprite_path);

                        if !registered_sprites.contains(&sprite_id) {
                            let companions = self.resolve_pbr_companions(&tex_loc);
                            if companions.albedo.is_some() {
                                registered_sprites.insert(sprite_id.clone());
                                results.push(DiscoveredSprite {
                                    sprite_id,
                                    texture_location: tex_loc,
                                    raw_albedo: companions.albedo,
                                    raw_normal: companions.normal,
                                    raw_specular: companions.specular,
                                    raw_overlay: companions.overlay,
                                    raw_emissive: companions.emissive,
                                    metadata: companions.mcmeta,
                                    emissive_metadata: companions.emissive_mcmeta,
                                });
                            }
                        }
                    }
                }

                AtlasSource::Single { resource, sprite } => {
                    let sprite_id = sprite.clone().unwrap_or_else(|| resource.clone());
                    if !registered_sprites.contains(&sprite_id) {
                        let companions = self.resolve_pbr_companions(resource);
                        if companions.albedo.is_some() {
                            registered_sprites.insert(sprite_id.clone());
                            results.push(DiscoveredSprite {
                                sprite_id,
                                texture_location: resource.clone(),
                                raw_albedo: companions.albedo,
                                raw_normal: companions.normal,
                                raw_specular: companions.specular,
                                raw_overlay: companions.overlay,
                                raw_emissive: companions.emissive,
                                metadata: companions.mcmeta,
                                emissive_metadata: companions.emissive_mcmeta,
                            });
                        }
                    }
                }

                AtlasSource::PalettedPermutations { .. } => {
                    // PalettedPermutations are dynamically baked directly by AtlasBuilder
                }

                AtlasSource::Unstitch {
                    resource,
                    divisor_x: _,
                    divisor_y: _,
                    regions,
                } => {
                    let companions = self.resolve_pbr_companions(resource);
                    for region in regions {
                        if !registered_sprites.contains(&region.sprite) {
                            registered_sprites.insert(region.sprite.clone());
                            results.push(DiscoveredSprite {
                                sprite_id: region.sprite.clone(),
                                texture_location: resource.clone(),
                                raw_albedo: companions.albedo.clone(),
                                raw_normal: companions.normal.clone(),
                                raw_specular: companions.specular.clone(),
                                raw_overlay: companions.overlay.clone(),
                                raw_emissive: companions.emissive.clone(),
                                metadata: None,
                                emissive_metadata: None,
                            });
                        }
                    }
                }

                AtlasSource::Filter { pattern } => {
                    // Filter matching entries
                    results.retain(|s| {
                        let ns_match = pattern
                            .namespace
                            .as_ref()
                            .is_none_or(|ns_pat| s.sprite_id.namespace.contains(ns_pat));
                        let path_match = pattern
                            .path
                            .as_ref()
                            .is_none_or(|p_pat| s.sprite_id.path.contains(p_pat));
                        !(ns_match && path_match)
                    });
                }
            }
        }

        Ok(results)
    }

    /// Collect atlas sprites including all sub-tiles referenced by active CTM rules.
    pub fn collect_sprites_including_ctm(
        &self,
        definition: &AtlasDefinition,
        ctm_rules: &[crate::ctm::CtmRule],
    ) -> Result<Vec<DiscoveredSprite>, ResourceError> {
        let mut results = self.collect_sprites_for_atlas(definition)?;
        let mut registered_sprites: HashSet<ResourceLocation> =
            results.iter().map(|s| s.sprite_id.clone()).collect();

        for rule in ctm_rules {
            for tile_loc in rule.tiles.iter().flatten() {
                if !registered_sprites.contains(tile_loc) {
                    let companions = self.resolve_pbr_companions(tile_loc);
                    if companions.albedo.is_some() {
                        registered_sprites.insert(tile_loc.clone());
                        results.push(DiscoveredSprite {
                            sprite_id: tile_loc.clone(),
                            texture_location: tile_loc.clone(),
                            raw_albedo: companions.albedo,
                            raw_normal: companions.normal,
                            raw_specular: companions.specular,
                            raw_overlay: companions.overlay,
                            raw_emissive: companions.emissive,
                            metadata: companions.mcmeta,
                            emissive_metadata: companions.emissive_mcmeta,
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}
