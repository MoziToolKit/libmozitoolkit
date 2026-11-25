use glam::IVec3;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// One `.mca` file path required to cover a spatial selection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlannedRegionFile {
    pub rx: i32,
    pub rz: i32,
    pub path: String,
}

/// One `.mcc` external chunk stream path intersecting a spatial selection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlannedExternalChunk {
    pub cx: i32,
    pub cz: i32,
    pub path: String,
}

/// Complete set of dimension region/external chunk files intersecting a selection bounding box.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RegionSelectionPlan {
    pub regions: Vec<PlannedRegionFile>,
    pub external_chunks: Vec<PlannedExternalChunk>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BoxSelectionBounds {
    pub true_min: IVec3,
    pub true_max: IVec3,
    pub min_sec_x: i32,
    pub max_sec_x: i32,
    pub min_sec_y: i32,
    pub max_sec_y: i32,
    pub min_sec_z: i32,
    pub max_sec_z: i32,
    pub min_reg_x: i32,
    pub max_reg_x: i32,
    pub min_reg_z: i32,
    pub max_reg_z: i32,
    pub total_chunks: usize,
}

impl BoxSelectionBounds {
    pub(crate) fn from_corners(min_block: IVec3, max_block: IVec3) -> Self {
        let true_min = min_block.min(max_block);
        let true_max = min_block.max(max_block);
        let min_sec_x = true_min.x >> 4;
        let max_sec_x = true_max.x >> 4;
        let min_sec_y = true_min.y >> 4;
        let max_sec_y = true_max.y >> 4;
        let min_sec_z = true_min.z >> 4;
        let max_sec_z = true_max.z >> 4;
        let total_chunks =
            (((max_sec_x - min_sec_x + 1).max(1)) * ((max_sec_z - min_sec_z + 1).max(1))) as usize;
        Self {
            true_min,
            true_max,
            min_sec_x,
            max_sec_x,
            min_sec_y,
            max_sec_y,
            min_sec_z,
            max_sec_z,
            min_reg_x: min_sec_x >> 5,
            max_reg_x: max_sec_x >> 5,
            min_reg_z: min_sec_z >> 5,
            max_reg_z: max_sec_z >> 5,
            total_chunks,
        }
    }

    #[inline]
    pub(crate) fn region_chunk_range_x(&self, rx: i32) -> RangeInclusive<i32> {
        (rx * 32).max(self.min_sec_x)..=(rx * 32 + 31).min(self.max_sec_x)
    }

    #[inline]
    pub(crate) fn region_chunk_range_z(&self, rz: i32) -> RangeInclusive<i32> {
        (rz * 32).max(self.min_sec_z)..=(rz * 32 + 31).min(self.max_sec_z)
    }
}

pub(crate) fn discover_dimensions_from_paths<'a, I>(rel_paths: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut prefixes = Vec::new();
    for raw in rel_paths {
        if let Some(dir) = extract_region_dir_prefix(raw) {
            if !prefixes.contains(&dir) {
                prefixes.push(dir);
            }
        }
    }

    let mut dims = Vec::new();
    if prefixes
        .iter()
        .any(|p| p == "region" || p == "dimensions/minecraft/overworld/region")
    {
        dims.push("overworld".to_string());
    }
    if prefixes
        .iter()
        .any(|p| p == "dimensions/minecraft/the_nether/region" || p == "DIM-1/region")
    {
        dims.push("the_nether".to_string());
    }
    if prefixes
        .iter()
        .any(|p| p == "dimensions/minecraft/the_end/region" || p == "DIM1/region")
    {
        dims.push("the_end".to_string());
    }

    let mut custom = Vec::new();
    for p in &prefixes {
        if let Some(rest) = p
            .strip_prefix("dimensions/")
            .and_then(|s| s.strip_suffix("/region"))
        {
            if let Some((ns, d_name)) = rest.split_once('/') {
                let full_key = if ns == "minecraft" {
                    d_name.to_string()
                } else {
                    format!("{ns}:{d_name}")
                };
                if !dims.contains(&full_key) && !custom.contains(&full_key) {
                    custom.push(full_key);
                }
            }
        }
    }
    custom.sort();
    dims.extend(custom);

    if dims.is_empty() {
        dims.push("overworld".to_string());
    }
    dims
}

pub(crate) fn plan_dimension_region_files<'a, I>(
    dimension: &str,
    min_block: IVec3,
    max_block: IVec3,
    rel_paths: I,
) -> RegionSelectionPlan
where
    I: IntoIterator<Item = &'a str>,
{
    let sel = BoxSelectionBounds::from_corners(min_block, max_block);
    let paths: Vec<String> = rel_paths
        .into_iter()
        .map(normalize_save_rel_path)
        .filter(|p| !p.is_empty())
        .collect();

    let Some(region_prefix) = resolve_dimension_region_prefix(dimension, &paths) else {
        return RegionSelectionPlan::default();
    };

    let mut mca_map: BTreeMap<(i32, i32), String> = BTreeMap::new();
    let mut mcc_list: Vec<PlannedExternalChunk> = Vec::new();

    for path in paths {
        let Some((parent, file_name)) = path.rsplit_once('/') else {
            continue;
        };
        if parent != region_prefix {
            continue;
        }
        if let Some((rx, rz)) = parse_coord_filename(file_name, "r.", ".mca") {
            if (sel.min_reg_x..=sel.max_reg_x).contains(&rx)
                && (sel.min_reg_z..=sel.max_reg_z).contains(&rz)
            {
                mca_map.entry((rx, rz)).or_insert(path);
            }
        } else if let Some((cx, cz)) = parse_coord_filename(file_name, "c.", ".mcc") {
            if (sel.min_sec_x..=sel.max_sec_x).contains(&cx)
                && (sel.min_sec_z..=sel.max_sec_z).contains(&cz)
            {
                mcc_list.push(PlannedExternalChunk { cx, cz, path });
            }
        }
    }

    let regions = mca_map
        .into_iter()
        .map(|((rx, rz), path)| PlannedRegionFile { rx, rz, path })
        .collect();

    RegionSelectionPlan {
        regions,
        external_chunks: mcc_list,
    }
}

fn normalize_save_rel_path(raw: &str) -> String {
    let slash = raw.replace('\\', "/");
    slash
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn parse_coord_filename(file_name: &str, prefix: &str, suffix: &str) -> Option<(i32, i32)> {
    let body = file_name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let (x_str, z_str) = body.split_once('.')?;
    let x = x_str.parse::<i32>().ok()?;
    let z = z_str.parse::<i32>().ok()?;
    Some((x, z))
}

fn extract_region_dir_prefix(raw: &str) -> Option<String> {
    let norm = normalize_save_rel_path(raw);
    let (parent, file_name) = norm.rsplit_once('/')?;
    parse_coord_filename(file_name, "r.", ".mca")?;
    if parent == "region" || parent.ends_with("/region") {
        Some(parent.to_string())
    } else {
        None
    }
}

fn resolve_dimension_region_prefix(dimension: &str, rel_paths: &[String]) -> Option<String> {
    let mut region_dirs = Vec::new();
    for path in rel_paths {
        if let Some(dir) = extract_region_dir_prefix(path) {
            if !region_dirs.contains(&dir) {
                region_dirs.push(dir);
            }
        }
    }
    let has = |candidate: &str| region_dirs.iter().any(|d| d == candidate);

    match dimension.trim().to_lowercase().as_str() {
        "overworld" | "minecraft:overworld" => {
            if has("region") {
                return Some("region".to_string());
            }
            if has("dimensions/minecraft/overworld/region") {
                return Some("dimensions/minecraft/overworld/region".to_string());
            }
        }
        "nether" | "the_nether" | "minecraft:the_nether" | "dim-1" => {
            if has("dimensions/minecraft/the_nether/region") {
                return Some("dimensions/minecraft/the_nether/region".to_string());
            }
            if has("DIM-1/region") {
                return Some("DIM-1/region".to_string());
            }
        }
        "end" | "the_end" | "minecraft:the_end" | "dim1" => {
            if has("dimensions/minecraft/the_end/region") {
                return Some("dimensions/minecraft/the_end/region".to_string());
            }
            if has("DIM1/region") {
                return Some("DIM1/region".to_string());
            }
        }
        custom => {
            let candidate = if let Some((ns, d_name)) = custom.split_once(':') {
                format!("dimensions/{ns}/{d_name}/region")
            } else {
                format!("dimensions/minecraft/{custom}/region")
            };
            if has(&candidate) {
                return Some(candidate);
            }
        }
    }

    if has("region") {
        Some("region".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_and_plan_dimensions_from_paths() {
        let paths = [
            "level.dat",
            "dimensions/minecraft/overworld/region/r.0.0.mca",
            "dimensions/minecraft/overworld/region/r.-1.-1.mca",
            "dimensions/minecraft/overworld/region/r.10.10.mca",
            "dimensions/minecraft/overworld/region/c.-1.-1.mcc",
            "dimensions/minecraft/the_nether/region/r.0.0.mca",
            "dimensions/custom/void/region/r.0.0.mca",
        ];
        assert_eq!(
            discover_dimensions_from_paths(paths.iter().copied()),
            vec!["overworld", "the_nether", "custom:void"]
        );

        let plan = plan_dimension_region_files(
            "overworld",
            IVec3::new(-64, -64, -64),
            IVec3::new(64, 320, 64),
            paths.iter().copied(),
        );
        assert_eq!(
            plan.regions,
            vec![
                PlannedRegionFile {
                    rx: -1,
                    rz: -1,
                    path: "dimensions/minecraft/overworld/region/r.-1.-1.mca".to_string(),
                },
                PlannedRegionFile {
                    rx: 0,
                    rz: 0,
                    path: "dimensions/minecraft/overworld/region/r.0.0.mca".to_string(),
                },
            ]
        );
        assert_eq!(
            plan.external_chunks,
            vec![PlannedExternalChunk {
                cx: -1,
                cz: -1,
                path: "dimensions/minecraft/overworld/region/c.-1.-1.mcc".to_string(),
            }]
        );
    }
}
