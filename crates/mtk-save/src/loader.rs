use glam::IVec3;
use mtk_core::{ProgressCallback, ProgressReport, ProgressThrottler};
use mtk_voxel::{VoxelStorage, VoxelWriter};
use std::path::{Path, PathBuf};

use crate::chunk::ChunkParser;
use crate::entity::EntityParser;
use crate::error::SaveError;
use crate::level::LevelData;
use crate::region::RegionFile;
use mtk_model::EntityInstance;

/// High-level loader providing automatic Minecraft world directory discovery,
/// metadata extraction, and on-demand spatial bounding box slicing.
pub struct SaveLoader;

impl SaveLoader {
    /// Attempts to locate `level.dat` given a root directory or child path.
    pub fn find_level_dat(path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            if path.file_name().is_some_and(|f| f == "level.dat") {
                return Some(path.to_path_buf());
            }
            if let Some(parent) = path.parent() {
                return Self::find_level_dat(parent);
            }
            return None;
        }

        // Direct candidate
        let direct = path.join("level.dat");
        if direct.exists() {
            return Some(direct);
        }

        // Check parent directory
        if let Some(parent) = path.parent() {
            let parent_dat = parent.join("level.dat");
            if parent_dat.exists() {
                return Some(parent_dat);
            }
        }

        None
    }

    /// Reads `level.dat` metadata from a save directory or file path.
    pub fn read_level_data(path: &Path) -> Result<LevelData, SaveError> {
        let level_dat = Self::find_level_dat(path)
            .ok_or_else(|| SaveError::LevelDatNotFound(path.to_path_buf()))?;
        LevelData::read_from_file(&level_dat)
    }

    /// Discovers all available dimensions in the specified world folder.
    pub fn discover_dimensions(save_dir: &Path) -> Vec<String> {
        let mut dims = Vec::new();

        // 1. Overworld check
        if save_dir.join("region").exists()
            || save_dir
                .join("dimensions/minecraft/overworld/region")
                .exists()
        {
            dims.push("overworld".to_string());
        }

        // 2. The Nether check (1.18+ dimension folder layout)
        if save_dir
            .join("dimensions/minecraft/the_nether/region")
            .exists()
        {
            dims.push("the_nether".to_string());
        }

        // 3. The End check (1.18+ dimension folder layout)
        if save_dir
            .join("dimensions/minecraft/the_end/region")
            .exists()
        {
            dims.push("the_end".to_string());
        }

        // 4. Custom dimensions under dimensions/*/*/region
        let dimensions_dir = save_dir.join("dimensions");
        if dimensions_dir.is_dir() {
            if let Ok(namespaces) = std::fs::read_dir(&dimensions_dir) {
                for ns_entry in namespaces.flatten() {
                    let ns_path = ns_entry.path();
                    if ns_path.is_dir() {
                        let ns_name = ns_entry.file_name().to_string_lossy().into_owned();
                        if let Ok(dim_entries) = std::fs::read_dir(&ns_path) {
                            for d_entry in dim_entries.flatten() {
                                let d_path = d_entry.path();
                                if d_path.join("region").exists() {
                                    let d_name = d_entry.file_name().to_string_lossy().into_owned();
                                    let full_key = if ns_name == "minecraft" {
                                        d_name
                                    } else {
                                        format!("{}:{}", ns_name, d_name)
                                    };
                                    if !dims.contains(&full_key) {
                                        dims.push(full_key);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if dims.is_empty() {
            dims.push("overworld".to_string());
        }

        dims
    }

    /// Resolves the region directory for a given dimension name.
    pub fn resolve_dimension_region_dir(save_dir: &Path, dimension: &str) -> Option<PathBuf> {
        let dim_clean = dimension.trim().to_lowercase();

        // If the path itself is already a region folder
        if save_dir.is_dir() && save_dir.file_name().is_some_and(|f| f == "region") {
            return Some(save_dir.to_path_buf());
        }

        match dim_clean.as_str() {
            "overworld" | "minecraft:overworld" => {
                let p1 = save_dir.join("region");
                if p1.is_dir() {
                    return Some(p1);
                }
                let p2 = save_dir.join("dimensions/minecraft/overworld/region");
                if p2.is_dir() {
                    return Some(p2);
                }
            }
            "nether" | "the_nether" | "minecraft:the_nether" => {
                let p = save_dir.join("dimensions/minecraft/the_nether/region");
                if p.is_dir() {
                    return Some(p);
                }
            }
            "end" | "the_end" | "minecraft:the_end" => {
                let p = save_dir.join("dimensions/minecraft/the_end/region");
                if p.is_dir() {
                    return Some(p);
                }
            }
            custom => {
                let parts: Vec<&str> = custom.split(':').collect();
                if parts.len() == 2 {
                    let p = save_dir.join(format!("dimensions/{}/{}/region", parts[0], parts[1]));
                    if p.is_dir() {
                        return Some(p);
                    }
                } else {
                    let p = save_dir.join(format!("dimensions/minecraft/{}/region", custom));
                    if p.is_dir() {
                        return Some(p);
                    }
                }
            }
        }

        // Fallback: check direct region folder
        let direct_region = save_dir.join("region");
        if direct_region.is_dir() {
            Some(direct_region)
        } else {
            None
        }
    }

    /// Resolves the entities directory for a given dimension name.
    pub fn resolve_dimension_entities_dir(save_dir: &Path, dimension: &str) -> Option<PathBuf> {
        let dim_clean = dimension.trim().to_lowercase();

        // If the path itself is already an entities folder
        if save_dir.is_dir() && save_dir.file_name().is_some_and(|f| f == "entities") {
            return Some(save_dir.to_path_buf());
        }

        match dim_clean.as_str() {
            "overworld" | "minecraft:overworld" => {
                let p1 = save_dir.join("entities");
                if p1.is_dir() {
                    return Some(p1);
                }
                let p2 = save_dir.join("dimensions/minecraft/overworld/entities");
                if p2.is_dir() {
                    return Some(p2);
                }
            }
            "nether" | "the_nether" | "minecraft:the_nether" => {
                let p = save_dir.join("dimensions/minecraft/the_nether/entities");
                if p.is_dir() {
                    return Some(p);
                }
            }
            "end" | "the_end" | "minecraft:the_end" => {
                let p = save_dir.join("dimensions/minecraft/the_end/entities");
                if p.is_dir() {
                    return Some(p);
                }
            }
            custom => {
                let parts: Vec<&str> = custom.split(':').collect();
                if parts.len() == 2 {
                    let p = save_dir.join(format!("dimensions/{}/{}/entities", parts[0], parts[1]));
                    if p.is_dir() {
                        return Some(p);
                    }
                } else {
                    let p = save_dir.join(format!("dimensions/minecraft/{}/entities", custom));
                    if p.is_dir() {
                        return Some(p);
                    }
                }
            }
        }

        // Fallback: check direct entities folder
        let direct_entities = save_dir.join("entities");
        if direct_entities.is_dir() {
            Some(direct_entities)
        } else {
            None
        }
    }

    /// Loads only the required chunk sections within `[min_block, max_block]` into a target `VoxelStorage`.
    ///
    /// Performs direct-seek MCA reads, uncompressing only intersecting chunks and Y sections.
    /// Returns the number of loaded chunk sections.
    pub fn load_box_into_storage(
        save_dir: &Path,
        dimension: &str,
        min_block: IVec3,
        max_block: IVec3,
        storage: &mut VoxelStorage,
    ) -> Result<usize, SaveError> {
        Self::load_box_into_storage_with_progress(
            save_dir, dimension, min_block, max_block, storage, None,
        )
    }

    /// Loads only the required chunk sections within `[min_block, max_block]` into a target `VoxelStorage` with progress reporting.
    pub fn load_box_into_storage_with_progress(
        save_dir: &Path,
        dimension: &str,
        min_block: IVec3,
        max_block: IVec3,
        storage: &mut VoxelStorage,
        progress: Option<ProgressCallback>,
    ) -> Result<usize, SaveError> {
        let (sections, _) = Self::load_box_with_entities(
            save_dir, dimension, min_block, max_block, storage, progress,
        )?;
        Ok(sections)
    }

    /// Loads chunk sections and extracts decoupled continuous visual entities within `[min_block, max_block]`.
    pub fn load_box_with_entities(
        save_dir: &Path,
        dimension: &str,
        min_block: IVec3,
        max_block: IVec3,
        storage: &mut VoxelStorage,
        progress: Option<ProgressCallback>,
    ) -> Result<(usize, Vec<EntityInstance>), SaveError> {
        let region_dir = Self::resolve_dimension_region_dir(save_dir, dimension)
            .ok_or_else(|| SaveError::RegionDirNotFound(save_dir.to_path_buf()))?;
        let entities_dir = Self::resolve_dimension_entities_dir(save_dir, dimension);

        // 1. Calculate bounding box ranges
        let true_min = min_block.min(max_block);
        let true_max = min_block.max(max_block);

        let min_sec_x = true_min.x >> 4;
        let max_sec_x = true_max.x >> 4;
        let min_sec_y = true_min.y >> 4;
        let max_sec_y = true_max.y >> 4;
        let min_sec_z = true_min.z >> 4;
        let max_sec_z = true_max.z >> 4;

        let min_reg_x = min_sec_x >> 5;
        let max_reg_x = max_sec_x >> 5;
        let min_reg_z = min_sec_z >> 5;
        let max_reg_z = max_sec_z >> 5;

        let total_chunks =
            (((max_sec_x - min_sec_x + 1).max(1)) * ((max_sec_z - min_sec_z + 1).max(1))) as usize;
        let throttler = ProgressThrottler::new("load_chunks", total_chunks, progress)
            .with_prefix("Loading chunk");

        let mut loaded_sections = 0usize;
        let mut loaded_entities = Vec::new();

        // 2. Iterate only over regions that overlap the bounding box
        for rx in min_reg_x..=max_reg_x {
            for rz in min_reg_z..=max_reg_z {
                let mca_path = region_dir.join(format!("r.{}.{}.mca", rx, rz));
                if !mca_path.exists() {
                    let chunk_start_x = (rx * 32).max(min_sec_x);
                    let chunk_end_x = (rx * 32 + 31).min(max_sec_x);
                    let chunk_start_z = (rz * 32).max(min_sec_z);
                    let chunk_end_z = (rz * 32 + 31).min(max_sec_z);
                    let skipped = ((chunk_end_x - chunk_start_x + 1).max(0)
                        * (chunk_end_z - chunk_start_z + 1).max(0))
                        as usize;
                    throttler.inc_by(skipped);
                    continue;
                }

                let mut region_file = match RegionFile::open(&mca_path) {
                    Ok(rf) => rf,
                    Err(_) => {
                        let chunk_start_x = (rx * 32).max(min_sec_x);
                        let chunk_end_x = (rx * 32 + 31).min(max_sec_x);
                        let chunk_start_z = (rz * 32).max(min_sec_z);
                        let chunk_end_z = (rz * 32 + 31).min(max_sec_z);
                        let skipped = ((chunk_end_x - chunk_start_x + 1).max(0)
                            * (chunk_end_z - chunk_start_z + 1).max(0))
                            as usize;
                        throttler.inc_by(skipped);
                        continue;
                    }
                };

                let chunk_start_x = (rx * 32).max(min_sec_x);
                let chunk_end_x = (rx * 32 + 31).min(max_sec_x);
                let chunk_start_z = (rz * 32).max(min_sec_z);
                let chunk_end_z = (rz * 32 + 31).min(max_sec_z);

                for cx in chunk_start_x..=chunk_end_x {
                    for cz in chunk_start_z..=chunk_end_z {
                        if !region_file.has_chunk(cx, cz) {
                            throttler.inc();
                            continue;
                        }

                        let decompressed = match region_file.read_chunk_decompressed(cx, cz)? {
                            Some(d) => d,
                            None => {
                                throttler.inc();
                                continue;
                            }
                        };

                        let sections =
                            ChunkParser::parse_chunk(&decompressed, min_sec_y, max_sec_y)?;

                        for mut section in sections {
                            let sec_x = section.coord.x;
                            let sec_y = section.coord.y;
                            let sec_z = section.coord.z;

                            // Filter block entities strictly within [true_min, true_max] bounding box
                            if !section.block_entities.is_empty() {
                                let sec_base_x = sec_x * 16;
                                let sec_base_y = sec_y * 16;
                                let sec_base_z = sec_z * 16;
                                section.block_entities.retain(|&local_idx, _| {
                                    let lx = ((local_idx >> 8) & 15) as i32;
                                    let ly = ((local_idx >> 4) & 15) as i32;
                                    let lz = (local_idx & 15) as i32;
                                    let wx = sec_base_x + lx;
                                    let wy = sec_base_y + ly;
                                    let wz = sec_base_z + lz;
                                    wx >= true_min.x
                                        && wx <= true_max.x
                                        && wy >= true_min.y
                                        && wy <= true_max.y
                                        && wz >= true_min.z
                                        && wz <= true_max.z
                                });
                            }

                            storage.set_section(sec_x, sec_y, sec_z, section);
                            loaded_sections += 1;
                        }

                        // Extract legacy entities from region chunk NBT if separate entities dir is absent
                        if entities_dir.is_none() {
                            if let Ok(entities) =
                                EntityParser::parse_entities(&decompressed, true_min, true_max)
                            {
                                loaded_entities.extend(entities);
                            }
                        }

                        throttler.inc();
                    }
                }
            }
        }

        // 3. Scan and extract visual entities from separate entities/*.mca directory (1.17+ format)
        if let Some(ref ent_dir) = entities_dir {
            // Expand entity chunk bounds by 1 chunk in X and Z to capture entities
            // whose Pos placed them in adjacent chunks but whose AABB overlaps the selection
            let min_ent_sec_x = min_sec_x - 1;
            let max_ent_sec_x = max_sec_x + 1;
            let min_ent_sec_z = min_sec_z - 1;
            let max_ent_sec_z = max_sec_z + 1;

            let min_ent_reg_x = min_ent_sec_x.div_euclid(32);
            let max_ent_reg_x = max_ent_sec_x.div_euclid(32);
            let min_ent_reg_z = min_ent_sec_z.div_euclid(32);
            let max_ent_reg_z = max_ent_sec_z.div_euclid(32);

            for rx in min_ent_reg_x..=max_ent_reg_x {
                for rz in min_ent_reg_z..=max_ent_reg_z {
                    let mca_path = ent_dir.join(format!("r.{}.{}.mca", rx, rz));
                    if !mca_path.exists() {
                        continue;
                    }

                    let mut region_file = match RegionFile::open(&mca_path) {
                        Ok(rf) => rf,
                        Err(_) => continue,
                    };

                    let chunk_start_x = (rx * 32).max(min_ent_sec_x);
                    let chunk_end_x = (rx * 32 + 31).min(max_ent_sec_x);
                    let chunk_start_z = (rz * 32).max(min_ent_sec_z);
                    let chunk_end_z = (rz * 32 + 31).min(max_ent_sec_z);

                    for cx in chunk_start_x..=chunk_end_x {
                        for cz in chunk_start_z..=chunk_end_z {
                            if !region_file.has_chunk(cx, cz) {
                                continue;
                            }

                            let decompressed = match region_file.read_chunk_decompressed(cx, cz) {
                                Ok(Some(d)) => d,
                                _ => continue,
                            };

                            if let Ok(entities) =
                                EntityParser::parse_entities(&decompressed, true_min, true_max)
                            {
                                loaded_entities.extend(entities);
                            }
                        }
                    }
                }
            }
        }

        // 4. Set the active world bounds on VoxelStorage
        let size_x = (true_max.x - true_min.x + 1).max(1);
        let size_y = (true_max.y - true_min.y + 1).max(1);
        let size_z = (true_max.z - true_min.z + 1).max(1);
        storage.set_bounds(true_min.x, true_min.y, true_min.z, size_x, size_y, size_z);

        // 5. Derive biome metadata through the single shared implementation
        storage.finalize_biome_metadata();

        if let Some(cb) = progress {
            cb(ProgressReport::new(
                "load_chunks",
                total_chunks,
                total_chunks,
                format!(
                    "Loaded {} chunk sections from {} chunks ({} entities)",
                    loaded_sections,
                    total_chunks,
                    loaded_entities.len()
                ),
            ));
        }

        Ok((loaded_sections, loaded_entities))
    }
}
