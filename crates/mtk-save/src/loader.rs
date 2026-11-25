use glam::IVec3;
use mtk_core::{ProgressCallback, ProgressReport, ProgressThrottler};
use mtk_voxel::{SectionStorage, VoxelStorage, VoxelWriter};
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use crate::chunk::ChunkParser;
use crate::error::SaveError;
use crate::level::LevelData;
use crate::plan::{
    discover_dimensions_from_paths, plan_dimension_region_files, BoxSelectionBounds,
    RegionSelectionPlan,
};
use crate::region::RegionFile;

/// Raw in-memory bytes for a single Anvil `.mca` region file at `(rx, rz)`.
#[derive(Debug, Clone, Copy)]
pub struct RegionBytes<'a> {
    pub rx: i32,
    pub rz: i32,
    pub data: &'a [u8],
}

/// High-level loader providing automatic Minecraft world directory discovery,
/// metadata extraction, and on-demand spatial bounding box slicing.
pub struct SaveLoader;

impl SaveLoader {
    /// Attempts to locate `level.dat` given a root directory or child path.
    pub(crate) fn find_level_dat(path: &Path) -> Option<PathBuf> {
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

        // 2. The Nether check (1.18+/26.x dimension folder layout and classic DIM-1 layout)
        if save_dir
            .join("dimensions/minecraft/the_nether/region")
            .exists()
            || save_dir.join("DIM-1/region").exists()
        {
            dims.push("the_nether".to_string());
        }

        // 3. The End check (1.18+/26.x dimension folder layout and classic DIM1 layout)
        if save_dir
            .join("dimensions/minecraft/the_end/region")
            .exists()
            || save_dir.join("DIM1/region").exists()
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

    /// Discovers all available dimensions from normalized relative save paths (browser / VFS use).
    pub fn discover_dimensions_from_paths<'a, I>(rel_paths: I) -> Vec<String>
    where
        I: IntoIterator<Item = &'a str>,
    {
        discover_dimensions_from_paths(rel_paths)
    }

    /// Resolves which `.mca` (and matching `.mcc`) relative paths inside a dimension intersect `[min_block, max_block]`.
    pub fn plan_dimension_region_files<'a, I>(
        dimension: &str,
        min_block: IVec3,
        max_block: IVec3,
        rel_paths: I,
    ) -> RegionSelectionPlan
    where
        I: IntoIterator<Item = &'a str>,
    {
        plan_dimension_region_files(dimension, min_block, max_block, rel_paths)
    }

    /// Resolves the region directory for a given dimension name.
    pub(crate) fn resolve_dimension_region_dir(
        save_dir: &Path,
        dimension: &str,
    ) -> Option<PathBuf> {
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
            "nether" | "the_nether" | "minecraft:the_nether" | "dim-1" => {
                let modern = save_dir.join("dimensions/minecraft/the_nether/region");
                if modern.is_dir() {
                    return Some(modern);
                }
                let classic = save_dir.join("DIM-1/region");
                if classic.is_dir() {
                    return Some(classic);
                }
            }
            "end" | "the_end" | "minecraft:the_end" | "dim1" => {
                let modern = save_dir.join("dimensions/minecraft/the_end/region");
                if modern.is_dir() {
                    return Some(modern);
                }
                let classic = save_dir.join("DIM1/region");
                if classic.is_dir() {
                    return Some(classic);
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
        let region_dir = Self::resolve_dimension_region_dir(save_dir, dimension)
            .ok_or_else(|| SaveError::RegionDirNotFound(save_dir.to_path_buf()))?;

        let sel = BoxSelectionBounds::from_corners(min_block, max_block);
        let throttler = ProgressThrottler::new("load_chunks", sel.total_chunks, progress)
            .with_prefix("Loading chunk");

        let mut plans: Vec<RegionPlan> = Vec::new();
        for rx in sel.min_reg_x..=sel.max_reg_x {
            for rz in sel.min_reg_z..=sel.max_reg_z {
                let mca_path = region_dir.join(format!("r.{}.{}.mca", rx, rz));
                if !mca_path.exists() {
                    continue;
                }
                plans.push(RegionPlan {
                    path: mca_path,
                    xs: sel.region_chunk_range_x(rx),
                    zs: sel.region_chunk_range_z(rz),
                });
            }
        }

        #[cfg(feature = "parallel")]
        let decoded: Result<Vec<Vec<(IVec3, SectionStorage)>>, SaveError> = {
            use rayon::prelude::*;
            mtk_core::threading::install(|| {
                plans
                    .par_iter()
                    .map(|plan| decode_region_file(plan, &sel))
                    .collect()
            })
        };

        #[cfg(not(feature = "parallel"))]
        let decoded: Result<Vec<Vec<(IVec3, SectionStorage)>>, SaveError> = plans
            .iter()
            .map(|plan| decode_region_file(plan, &sel))
            .collect();

        Ok(finalize_loaded_sections(
            storage, decoded?, &sel, &throttler, progress,
        ))
    }

    /// Loads chunk sections within `[min_block, max_block]` from caller-supplied in-memory `.mca` buffers.
    pub fn load_box_from_regions_into_storage(
        regions: &[RegionBytes<'_>],
        external_chunks: &HashMap<(i32, i32), Vec<u8>>,
        min_block: IVec3,
        max_block: IVec3,
        storage: &mut VoxelStorage,
        progress: Option<ProgressCallback>,
    ) -> Result<usize, SaveError> {
        let sel = BoxSelectionBounds::from_corners(min_block, max_block);
        let throttler = ProgressThrottler::new("load_chunks", sel.total_chunks, progress)
            .with_prefix("Loading chunk");

        let decode_one =
            |region: &RegionBytes<'_>| -> Result<Vec<(IVec3, SectionStorage)>, SaveError> {
                let label = format!("r.{}.{}.mca", region.rx, region.rz);
                let mut region_file =
                    match RegionFile::from_bytes(region.data, &label, external_chunks) {
                        Ok(rf) => rf,
                        Err(_) => return Ok(Vec::new()),
                    };
                decode_region_reader(
                    &mut region_file,
                    sel.region_chunk_range_x(region.rx),
                    sel.region_chunk_range_z(region.rz),
                    &sel,
                )
            };

        #[cfg(feature = "parallel")]
        let decoded: Result<Vec<Vec<(IVec3, SectionStorage)>>, SaveError> = {
            use rayon::prelude::*;
            mtk_core::threading::install(|| regions.par_iter().map(decode_one).collect())
        };

        #[cfg(not(feature = "parallel"))]
        let decoded: Result<Vec<Vec<(IVec3, SectionStorage)>>, SaveError> =
            regions.iter().map(decode_one).collect();

        Ok(finalize_loaded_sections(
            storage, decoded?, &sel, &throttler, progress,
        ))
    }
}

fn finalize_loaded_sections(
    storage: &mut VoxelStorage,
    decoded: Vec<Vec<(IVec3, SectionStorage)>>,
    sel: &BoxSelectionBounds,
    throttler: &ProgressThrottler,
    progress: Option<ProgressCallback>,
) -> usize {
    let mut loaded_sections = 0usize;
    for sections in decoded {
        for (coord, section) in sections {
            storage.set_section(coord.x, coord.y, coord.z, section);
            loaded_sections += 1;
        }
    }

    throttler.inc_by(sel.total_chunks);

    let size_x = (sel.true_max.x - sel.true_min.x + 1).max(1);
    let size_y = (sel.true_max.y - sel.true_min.y + 1).max(1);
    let size_z = (sel.true_max.z - sel.true_min.z + 1).max(1);
    storage.set_bounds(
        sel.true_min.x,
        sel.true_min.y,
        sel.true_min.z,
        size_x,
        size_y,
        size_z,
    );
    storage.finalize_biome_metadata();

    if let Some(cb) = progress {
        cb(ProgressReport::new(
            "load_chunks",
            sel.total_chunks,
            sel.total_chunks,
            format!(
                "Loaded {loaded_sections} chunk sections from {} chunks",
                sel.total_chunks
            ),
        ));
    }

    loaded_sections
}

/// One `.mca` file plus the chunk range of the selection that falls inside it.
struct RegionPlan {
    path: std::path::PathBuf,
    xs: RangeInclusive<i32>,
    zs: RangeInclusive<i32>,
}

fn decode_region_file(
    plan: &RegionPlan,
    sel: &BoxSelectionBounds,
) -> Result<Vec<(IVec3, SectionStorage)>, SaveError> {
    let mut region_file = match RegionFile::open(&plan.path) {
        Ok(region_file) => region_file,
        Err(_) => return Ok(Vec::new()),
    };
    decode_region_reader(&mut region_file, plan.xs.clone(), plan.zs.clone(), sel)
}

fn decode_region_reader<'a, R: Read + Seek>(
    region_file: &mut RegionFile<'a, R>,
    xs: RangeInclusive<i32>,
    zs: RangeInclusive<i32>,
    sel: &BoxSelectionBounds,
) -> Result<Vec<(IVec3, SectionStorage)>, SaveError> {
    let mut out: Vec<(IVec3, SectionStorage)> = Vec::new();
    for cx in xs {
        for cz in zs.clone() {
            if !region_file.has_chunk(cx, cz) {
                continue;
            }

            let decompressed = match region_file.read_chunk_decompressed(cx, cz)? {
                Some(bytes) => bytes,
                None => continue,
            };

            let sections = ChunkParser::parse_chunk(&decompressed, sel.min_sec_y, sel.max_sec_y)?;

            for mut section in sections {
                if !section.block_entities.is_empty() {
                    let sec_base_x = section.coord.x * 16;
                    let sec_base_y = section.coord.y * 16;
                    let sec_base_z = section.coord.z * 16;
                    section.block_entities.retain(|&local_idx, _| {
                        let lx = ((local_idx >> 8) & 15) as i32;
                        let ly = ((local_idx >> 4) & 15) as i32;
                        let lz = (local_idx & 15) as i32;
                        let wx = sec_base_x + lx;
                        let wy = sec_base_y + ly;
                        let wz = sec_base_z + lz;
                        wx >= sel.true_min.x
                            && wx <= sel.true_max.x
                            && wy >= sel.true_min.y
                            && wy <= sel.true_max.y
                            && wz >= sel.true_min.z
                            && wz <= sel.true_max.z
                    });
                }

                out.push((section.coord, section));
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_dimension_region_dir_classic_and_modern() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtk_save_resolve_{}_{}",
            std::process::id(),
            unique
        ));
        let dim_nether = root.join("DIM-1/region");
        let dim_end = root.join("DIM1/region");
        std::fs::create_dir_all(&dim_nether).unwrap();
        std::fs::create_dir_all(&dim_end).unwrap();

        assert_eq!(
            SaveLoader::resolve_dimension_region_dir(&root, "the_nether"),
            Some(dim_nether.clone())
        );
        assert_eq!(
            SaveLoader::resolve_dimension_region_dir(&root, "minecraft:the_nether"),
            Some(dim_nether)
        );
        assert_eq!(
            SaveLoader::resolve_dimension_region_dir(&root, "the_end"),
            Some(dim_end)
        );

        let modern_nether = root.join("dimensions/minecraft/the_nether/region");
        std::fs::create_dir_all(&modern_nether).unwrap();
        assert_eq!(
            SaveLoader::resolve_dimension_region_dir(&root, "the_nether"),
            Some(modern_nether)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
