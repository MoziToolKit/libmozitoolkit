use super::sprite::{
    is_companion_asset_path, DiscoveredSprite, PalettedPermutationSpec, PbrCompanions,
    UnstitchRegionSpec,
};
use super::ResourcePackStack;
use crate::atlas::{AtlasDefinition, AtlasSource};
use crate::error::ResourceError;
use crate::identifier::ResourceLocation;
use std::collections::{BTreeMap, HashMap};

enum PendingSprite {
    Direct {
        texture_location: ResourceLocation,
    },
    Unstitch {
        texture_location: ResourceLocation,
        region: UnstitchRegionSpec,
    },
    Paletted {
        texture_location: ResourceLocation,
        palette_key: ResourceLocation,
        permutation: ResourceLocation,
    },
}

impl ResourcePackStack {
    /// Scan all active resource packs and load all OptiFine / Continuity CTM rules.
    pub fn load_ctm_rules(&self) -> Result<Vec<crate::ctm::CtmRule>, ResourceError> {
        let mut rules = Vec::new();

        for file in self.list_asset_files("assets/") {
            if !file.ends_with(".properties") {
                continue;
            }
            if !file.contains("/optifine/ctm/") && !file.contains("/textures/") {
                continue;
            }

            if let Some(bytes) = self.open_asset_raw(&file)? {
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

        rules.sort_by_key(|r| std::cmp::Reverse(r.priority));
        Ok(rules)
    }

    /// Constructs an indexed `CtmSolver` containing all active CTM rules loaded from this stack.
    pub fn build_ctm_solver(&self) -> Result<crate::ctm::CtmSolver, ResourceError> {
        Ok(crate::ctm::CtmSolver::new(self.load_ctm_rules()?))
    }

    /// Collect and expand all Sprite references declared in an AtlasDefinition into discrete
    /// `DiscoveredSprite`s, matching `Minecraft_Source/.../atlas/SpriteSourceList.java`.
    ///
    /// Sources execute in list order against one shared `BTreeMap<ResourceLocation, PendingSprite>`:
    /// later sources overwrite earlier entries with the same `sprite_id`, and `Filter` removes
    /// matching entries collected so far.
    pub fn collect_sprites_for_atlas(
        &self,
        definition: &AtlasDefinition,
    ) -> Result<Vec<DiscoveredSprite>, ResourceError> {
        let all_files = self.list_asset_files("assets/");
        let texture_locs: Vec<ResourceLocation> = all_files
            .iter()
            .filter(|file| file.ends_with(".png") && !is_companion_asset_path(file))
            .filter_map(|file| ResourceLocation::from_asset_path(file, "textures", "png"))
            .collect();

        let mut pending: BTreeMap<ResourceLocation, PendingSprite> = BTreeMap::new();

        for source in &definition.sources {
            match source {
                AtlasSource::Directory {
                    source: dir_src,
                    prefix,
                } => {
                    let dir_prefix = if dir_src.is_empty() {
                        String::new()
                    } else {
                        format!("{dir_src}/")
                    };
                    for tex_loc in &texture_locs {
                        let matches = if dir_src.is_empty() {
                            true
                        } else {
                            tex_loc.path == *dir_src || tex_loc.path.starts_with(&dir_prefix)
                        };
                        if !matches {
                            continue;
                        }
                        let short = if dir_src.is_empty() {
                            tex_loc.path.as_str()
                        } else if tex_loc.path == *dir_src {
                            ""
                        } else {
                            tex_loc
                                .path
                                .strip_prefix(&dir_prefix)
                                .unwrap_or(&tex_loc.path)
                        };
                        let sprite_id =
                            ResourceLocation::new(&tex_loc.namespace, format!("{prefix}{short}"));
                        pending.insert(
                            sprite_id,
                            PendingSprite::Direct {
                                texture_location: tex_loc.clone(),
                            },
                        );
                    }
                }

                AtlasSource::Single { resource, sprite } => {
                    let sprite_id = sprite.clone().unwrap_or_else(|| resource.clone());
                    pending.insert(
                        sprite_id,
                        PendingSprite::Direct {
                            texture_location: resource.clone(),
                        },
                    );
                }

                AtlasSource::Unstitch {
                    resource,
                    divisor_x,
                    divisor_y,
                    regions,
                } => {
                    for region in regions {
                        pending.insert(
                            region.sprite.clone(),
                            PendingSprite::Unstitch {
                                texture_location: resource.clone(),
                                region: UnstitchRegionSpec {
                                    x: region.x,
                                    y: region.y,
                                    width: region.width,
                                    height: region.height,
                                    divisor_x: *divisor_x,
                                    divisor_y: *divisor_y,
                                },
                            },
                        );
                    }
                }

                AtlasSource::PalettedPermutations {
                    palette_key,
                    permutations,
                    textures,
                    separator,
                } => {
                    let mut sorted_perms: Vec<(&String, &ResourceLocation)> =
                        permutations.iter().collect();
                    sorted_perms.sort_by(|a, b| a.0.cmp(b.0));

                    for tex_loc in textures {
                        for (perm_name, perm_loc) in &sorted_perms {
                            let sprite_id = ResourceLocation::new(
                                &tex_loc.namespace,
                                format!("{}{}{}", tex_loc.path, separator, perm_name),
                            );
                            pending.insert(
                                sprite_id,
                                PendingSprite::Paletted {
                                    texture_location: tex_loc.clone(),
                                    palette_key: palette_key.clone(),
                                    permutation: (*perm_loc).clone(),
                                },
                            );
                        }
                    }
                }

                AtlasSource::Filter { pattern } => {
                    pending.retain(|sprite_id, _| !pattern.matches(sprite_id));
                }
            }
        }

        self.materialize_pending_sprites(pending)
    }

    /// Collect atlas sprites including all sub-tiles referenced by active CTM rules.
    pub fn collect_sprites_including_ctm(
        &self,
        definition: &AtlasDefinition,
        ctm_rules: &[crate::ctm::CtmRule],
    ) -> Result<Vec<DiscoveredSprite>, ResourceError> {
        let mut results = self.collect_sprites_for_atlas(definition)?;
        let mut seen: std::collections::HashSet<ResourceLocation> =
            results.iter().map(|s| s.sprite_id.clone()).collect();

        let mut extra_locs: Vec<ResourceLocation> = Vec::new();
        for rule in ctm_rules {
            for tile_loc in rule.tiles.iter().flatten() {
                if seen.insert(tile_loc.clone()) {
                    extra_locs.push(tile_loc.clone());
                }
            }
        }
        extra_locs.sort();

        for tile_loc in extra_locs {
            let companions = self.resolve_pbr_companions(&tile_loc)?;
            if companions.albedo.is_some() {
                results.push(DiscoveredSprite {
                    sprite_id: tile_loc.clone(),
                    texture_location: tile_loc,
                    raw_albedo: companions.albedo,
                    raw_normal: companions.normal,
                    raw_specular: companions.specular,
                    raw_overlay: companions.overlay,
                    raw_emissive: companions.emissive,
                    metadata: companions.mcmeta,
                    emissive_metadata: companions.emissive_mcmeta,
                    unstitch: None,
                    palette: None,
                });
            }
        }

        Ok(results)
    }

    fn materialize_pending_sprites(
        &self,
        pending: BTreeMap<ResourceLocation, PendingSprite>,
    ) -> Result<Vec<DiscoveredSprite>, ResourceError> {
        let mut companion_cache: HashMap<ResourceLocation, PbrCompanions> = HashMap::new();
        let mut raw_tex_cache: HashMap<ResourceLocation, Option<Vec<u8>>> = HashMap::new();
        let mut results = Vec::with_capacity(pending.len());

        for (sprite_id, entry) in pending {
            match entry {
                PendingSprite::Direct { texture_location } => {
                    let companions = resolve_cached(self, &mut companion_cache, &texture_location)?;
                    if companions.albedo.is_none() {
                        continue;
                    }
                    results.push(DiscoveredSprite {
                        sprite_id,
                        texture_location,
                        raw_albedo: companions.albedo.clone(),
                        raw_normal: companions.normal.clone(),
                        raw_specular: companions.specular.clone(),
                        raw_overlay: companions.overlay.clone(),
                        raw_emissive: companions.emissive.clone(),
                        metadata: companions.mcmeta.clone(),
                        emissive_metadata: companions.emissive_mcmeta.clone(),
                        unstitch: None,
                        palette: None,
                    });
                }
                PendingSprite::Unstitch {
                    texture_location,
                    region,
                } => {
                    let companions = resolve_cached(self, &mut companion_cache, &texture_location)?;
                    if companions.albedo.is_none() {
                        continue;
                    }
                    results.push(DiscoveredSprite {
                        sprite_id,
                        texture_location,
                        raw_albedo: companions.albedo.clone(),
                        raw_normal: companions.normal.clone(),
                        raw_specular: companions.specular.clone(),
                        raw_overlay: companions.overlay.clone(),
                        raw_emissive: companions.emissive.clone(),
                        metadata: None,
                        emissive_metadata: None,
                        unstitch: Some(region),
                        palette: None,
                    });
                }
                PendingSprite::Paletted {
                    texture_location,
                    palette_key,
                    permutation,
                } => {
                    let Some(key_bytes) = open_raw_cached(self, &mut raw_tex_cache, &palette_key)?
                    else {
                        continue;
                    };
                    let Some(perm_bytes) = open_raw_cached(self, &mut raw_tex_cache, &permutation)?
                    else {
                        continue;
                    };
                    let companions = resolve_cached(self, &mut companion_cache, &texture_location)?;
                    if companions.albedo.is_none() {
                        continue;
                    }
                    results.push(DiscoveredSprite {
                        sprite_id,
                        texture_location,
                        raw_albedo: companions.albedo.clone(),
                        raw_normal: companions.normal.clone(),
                        raw_specular: companions.specular.clone(),
                        raw_overlay: companions.overlay.clone(),
                        raw_emissive: companions.emissive.clone(),
                        metadata: companions.mcmeta.clone(),
                        emissive_metadata: companions.emissive_mcmeta.clone(),
                        unstitch: None,
                        palette: Some(PalettedPermutationSpec {
                            raw_palette_key: key_bytes,
                            raw_permutation: perm_bytes,
                        }),
                    });
                }
            }
        }

        Ok(results)
    }
}

fn resolve_cached<'a>(
    stack: &ResourcePackStack,
    cache: &'a mut HashMap<ResourceLocation, PbrCompanions>,
    loc: &ResourceLocation,
) -> Result<&'a PbrCompanions, ResourceError> {
    if !cache.contains_key(loc) {
        let companions = stack.resolve_pbr_companions(loc)?;
        cache.insert(loc.clone(), companions);
    }
    Ok(&cache[loc])
}

fn open_raw_cached(
    stack: &ResourcePackStack,
    cache: &mut HashMap<ResourceLocation, Option<Vec<u8>>>,
    loc: &ResourceLocation,
) -> Result<Option<Vec<u8>>, ResourceError> {
    if !cache.contains_key(loc) {
        let bytes = stack.open_texture_raw(loc)?;
        cache.insert(loc.clone(), bytes);
    }
    Ok(cache[loc].clone())
}
