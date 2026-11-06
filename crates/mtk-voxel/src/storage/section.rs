use std::collections::HashMap;

use glam::IVec3;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use mtk_core::constants::voxel::{
    block_index, padded_index, PADDED_SIZE, PADDED_VOLUME, SECTION_SIZE, SECTION_VOLUME,
};

/// Extracts canonical Minecraft blockstate identifier from raw state or JSON-wrapped string.
/// Delegates to `mtk_model::normalize_canonical_blockstate` as the canonical SSOT.
#[inline]
pub(crate) fn extract_canonical_state_str(raw_state: &str) -> &str {
    if raw_state.is_empty() || raw_state == "minecraft:air" {
        return "minecraft:air";
    }

    let mut res = raw_state;
    if let Some(stripped) = raw_state.strip_prefix("{\"state\":\"") {
        if let Some(end_idx) = stripped.find('"') {
            res = &stripped[..end_idx];
        }
    }

    mtk_model::normalize_canonical_blockstate(res)
}

/// 16x16x16 Chunk Section with dense palette-indexed storage and non-air tracking.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SectionStorage {
    /// Section coordinate in 16-block chunk grid (e.g. `[cx, cy, cz]`).
    pub coord: IVec3,
    /// Palette mapping index `u16` -> BlockState string (slot 0 is always "minecraft:air").
    pub palette: Vec<String>,
    /// Dense 4096-element palette-indexed array (`x * 256 + y * 16 + z`).
    #[cfg_attr(feature = "serde", serde(with = "serde_voxels"))]
    pub voxels: Box<[u16; SECTION_VOLUME]>,
    /// Number of non-air voxels currently in this section.
    pub non_air_count: u32,
    /// Optional 3D 4x4x4 quart paletted biomes for this section.
    #[cfg_attr(feature = "serde", serde(default))]
    pub biomes: Option<super::biome::SectionBiomes>,
    /// RFC v3.2: Sparse block entities in this 16x16x16 section.
    /// Key is local block index (0..4095: lx * 256 + ly * 16 + lz).
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_entities: HashMap<u16, mtk_model::BlockEntityData>,
}

#[cfg(feature = "serde")]
mod serde_voxels {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(
        voxels: &Box<[u16; SECTION_VOLUME]>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        voxels.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Box<[u16; SECTION_VOLUME]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<u16>::deserialize(deserializer)?;
        if vec.len() != SECTION_VOLUME {
            return Err(serde::de::Error::invalid_length(
                vec.len(),
                &"a 4096-element array",
            ));
        }
        let boxed_slice = vec.into_boxed_slice();
        let boxed_array: Box<[u16; SECTION_VOLUME]> = match boxed_slice.try_into() {
            Ok(arr) => arr,
            Err(_) => {
                return Err(serde::de::Error::custom(
                    "failed to convert boxed slice to fixed array",
                ))
            }
        };
        Ok(boxed_array)
    }
}

impl SectionStorage {
    /// Creates an empty section filled with `minecraft:air` at coordinate `coord`.
    pub fn new(coord: IVec3) -> Self {
        Self {
            coord,
            palette: vec!["minecraft:air".to_string()],
            voxels: Box::new([0u16; SECTION_VOLUME]),
            non_air_count: 0,
            biomes: None,
            block_entities: HashMap::new(),
        }
    }

    /// Resets the section to empty air.
    pub fn clear(&mut self) {
        self.palette.truncate(1);
        self.voxels.fill(0);
        self.non_air_count = 0;
        self.biomes = None;
        self.block_entities.clear();
    }

    /// Retrieves the optional block entity associated with a local voxel index (0..4095).
    #[inline]
    pub fn get_block_entity(&self, local_idx: u16) -> Option<&mtk_model::BlockEntityData> {
        self.block_entities.get(&local_idx)
    }

    /// Sets or removes a block entity at a local voxel index (0..4095).
    /// Automatically normalizes properties (e.g. banner base color from blockstate) if needed.
    #[inline]
    pub fn set_block_entity(&mut self, local_idx: u16, data: Option<mtk_model::BlockEntityData>) {
        if let Some(mut d) = data {
            if let mtk_model::BlockEntityData::Banner {
                ref mut base_color, ..
            } = d
            {
                let lx = ((local_idx >> 8) & 15) as usize;
                let ly = ((local_idx >> 4) & 15) as usize;
                let lz = (local_idx & 15) as usize;
                let state_str = self.get_local_state(lx, ly, lz);
                if let Some(resolved_color) =
                    mtk_model::banner::extract_banner_color_from_state(state_str)
                {
                    *base_color = resolved_color.to_string();
                }
            }
            self.block_entities.insert(local_idx, d);
        } else {
            self.block_entities.remove(&local_idx);
        }
    }

    /// Creates a section pre-populated from a 4096-element blockstate slice.
    pub fn from_slice(coord: IVec3, states: &[&str]) -> Self {
        let mut section = Self::new(coord);
        let mut palette_map = HashMap::<String, u16>::new();
        palette_map.insert("minecraft:air".to_string(), 0);

        let count = states.len().min(SECTION_VOLUME);
        for (idx, state) in states.iter().take(count).enumerate() {
            let state_canon = extract_canonical_state_str(state);
            let pal_idx = if let Some(&id) = palette_map.get(state_canon) {
                id
            } else {
                let id = section.palette.len() as u16;
                section.palette.push(state_canon.to_string());
                palette_map.insert(state_canon.to_string(), id);
                id
            };

            section.voxels[idx] = pal_idx;
            if pal_idx != 0 {
                section.non_air_count += 1;
            }
        }
        section
    }

    /// Checks if this section is known to be entirely empty (all air).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.non_air_count == 0
    }

    /// Returns the palette index at local section coordinate `(x, y, z)` in `0..16`.
    #[inline]
    pub fn get_local_idx(&self, x: usize, y: usize, z: usize) -> u16 {
        if x < SECTION_SIZE && y < SECTION_SIZE && z < SECTION_SIZE {
            self.voxels[block_index(x, y, z)]
        } else {
            0
        }
    }

    /// Returns the blockstate string at local section coordinate `(x, y, z)` in `0..16`.
    #[inline]
    pub fn get_local_state(&self, x: usize, y: usize, z: usize) -> &str {
        let pal_idx = self.get_local_idx(x, y, z) as usize;
        if pal_idx < self.palette.len() {
            &self.palette[pal_idx]
        } else {
            "minecraft:air"
        }
    }

    /// Sets the blockstate at local section coordinate `(x, y, z)` in `0..16`.
    /// Returns `true` if the voxel changed state.
    pub fn set_local(&mut self, x: usize, y: usize, z: usize, state: &str) -> bool {
        if x >= SECTION_SIZE || y >= SECTION_SIZE || z >= SECTION_SIZE {
            return false;
        }

        let state_canon = extract_canonical_state_str(state);
        let idx = block_index(x, y, z);
        let old_pal_idx = self.voxels[idx];
        let old_state = &self.palette[old_pal_idx as usize];

        if old_state == state_canon {
            return false;
        }

        // Find or insert in palette
        let new_pal_idx = if let Some(pos) = self.palette.iter().position(|s| s == state_canon) {
            pos as u16
        } else {
            let id = self.palette.len() as u16;
            self.palette.push(state_canon.to_string());
            id
        };

        if old_pal_idx == 0 && new_pal_idx != 0 {
            self.non_air_count += 1;
        } else if old_pal_idx != 0 && new_pal_idx == 0 {
            self.non_air_count = self.non_air_count.saturating_sub(1);
        }

        self.voxels[idx] = new_pal_idx;
        if !self.block_entities.is_empty() {
            self.block_entities.remove(&(idx as u16));
        }
        true
    }

    /// Builds a 18x18x18 padded array with a 1-voxel neighborhood apron.
    ///
    /// `get_neighbor` receives local coordinates `(lx, ly, lz)` in `-1..=16`
    /// for voxels outside the `0..16` section boundary.
    pub fn build_padded_array<'a, F>(&self, mut get_neighbor: F) -> PaddedVoxelArray
    where
        F: FnMut(i32, i32, i32) -> &'a str,
    {
        let mut palette_map = HashMap::<String, u16>::with_capacity(self.palette.len() + 16);
        let mut palette = self.palette.clone();
        for (i, st) in palette.iter().enumerate() {
            palette_map.insert(st.clone(), i as u16);
        }

        let mut last_cached_id: Option<u16> = None;
        let mut get_or_insert = |st: &str| -> u16 {
            if let Some(id) = last_cached_id {
                if let Some(existing) = palette.get(id as usize) {
                    if existing.as_str() == st {
                        return id;
                    }
                }
            }
            if let Some(&id) = palette_map.get(st) {
                last_cached_id = Some(id);
                id
            } else {
                let id = palette.len() as u16;
                let s = st.to_string();
                palette.push(s.clone());
                palette_map.insert(s, id);
                last_cached_id = Some(id);
                id
            }
        };

        let mut padded = vec![0u16; PADDED_VOLUME];

        // 1. Bulk copy the core 16x16x16 voxels row by row (256 copy_from_slice operations)
        for x in 0..16 {
            let px = x + 1;
            for y in 0..16 {
                let py = y + 1;
                let src_start = (x << 8) | (y << 4);
                let dst_start = px * (PADDED_SIZE * PADDED_SIZE) + py * PADDED_SIZE + 1;
                padded[dst_start..dst_start + 16]
                    .copy_from_slice(&self.voxels[src_start..src_start + 16]);
            }
        }

        // 2. Populate only the apron (the outer 1-voxel thick boundary, 1736 voxels)
        // Face -X (px = 0)
        for py in 0..PADDED_SIZE {
            let ly = py as i32 - 1;
            for pz in 0..PADDED_SIZE {
                let lz = pz as i32 - 1;
                let pad_idx = py * PADDED_SIZE + pz; // px = 0
                let n_state = get_neighbor(-1, ly, lz);
                padded[pad_idx] = get_or_insert(extract_canonical_state_str(n_state));
            }
        }

        // Face +X (px = 17)
        for py in 0..PADDED_SIZE {
            let ly = py as i32 - 1;
            for pz in 0..PADDED_SIZE {
                let lz = pz as i32 - 1;
                let pad_idx = 17 * (PADDED_SIZE * PADDED_SIZE) + py * PADDED_SIZE + pz; // px = 17
                let n_state = get_neighbor(16, ly, lz);
                padded[pad_idx] = get_or_insert(extract_canonical_state_str(n_state));
            }
        }

        // Intermediate X (px = 1..17)
        for px in 1..17 {
            let lx = px as i32 - 1;
            let px_offset = px * (PADDED_SIZE * PADDED_SIZE);

            // Face -Y (py = 0)
            for pz in 0..PADDED_SIZE {
                let lz = pz as i32 - 1;
                let pad_idx = px_offset + pz; // py = 0
                let n_state = get_neighbor(lx, -1, lz);
                padded[pad_idx] = get_or_insert(extract_canonical_state_str(n_state));
            }

            // Face +Y (py = 17)
            for pz in 0..PADDED_SIZE {
                let lz = pz as i32 - 1;
                let pad_idx = px_offset + 17 * PADDED_SIZE + pz; // py = 17
                let n_state = get_neighbor(lx, 16, lz);
                padded[pad_idx] = get_or_insert(extract_canonical_state_str(n_state));
            }

            // Face -Z and +Z (py = 1..17, pz = 0 and pz = 17)
            for py in 1..17 {
                let ly = py as i32 - 1;
                let py_offset = px_offset + py * PADDED_SIZE;

                // pz = 0
                let n_state_z0 = get_neighbor(lx, ly, -1);
                padded[py_offset] = get_or_insert(extract_canonical_state_str(n_state_z0));

                // pz = 17
                let n_state_z17 = get_neighbor(lx, ly, 16);
                padded[py_offset + 17] = get_or_insert(extract_canonical_state_str(n_state_z17));
            }
        }

        PaddedVoxelArray {
            coord: self.coord,
            palette,
            padded_voxels: padded,
            is_empty: self.is_empty(),
            biome_data: None,
            block_entities: self.block_entities.clone(),
        }
    }
}

/// 18x18x18 Padded Voxel Array for zero-branching face culling and neighbor inspection.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PaddedVoxelArray {
    /// Section coordinate in chunk grid.
    pub coord: IVec3,
    /// Unified palette for this section and its padded apron.
    pub palette: Vec<String>,
    /// 18x18x18 padded array indexed by `px * 324 + py * 18 + pz`.
    pub padded_voxels: Vec<u16>,
    /// Whether the core section contains only air.
    pub is_empty: bool,
    /// Precomputed smoothed biome columns (lx * 16 + lz in 0..256), if available.
    #[cfg_attr(feature = "serde", serde(default))]
    pub biome_data: Option<Vec<crate::biome::SmoothedBiomeColumn>>,
    /// RFC v3.2: Sparse block entities from the core 16x16x16 section.
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_entities: HashMap<u16, mtk_model::BlockEntityData>,
}

impl PaddedVoxelArray {
    /// Returns the blockstate string at padded coordinate `px, py, pz` in `0..18`.
    #[inline(always)]
    pub fn get_padded_state(&self, px: usize, py: usize, pz: usize) -> &str {
        let pad_idx = padded_index(px, py, pz);
        let pal_idx = self.padded_voxels[pad_idx] as usize;
        if pal_idx < self.palette.len() {
            &self.palette[pal_idx]
        } else {
            "minecraft:air"
        }
    }

    /// Returns the palette index at padded coordinate `px, py, pz` in `0..18`.
    #[inline(always)]
    pub fn get_padded_idx(&self, px: usize, py: usize, pz: usize) -> u16 {
        self.padded_voxels[padded_index(px, py, pz)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_storage_basic() {
        let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));
        assert!(sec.is_empty());

        sec.set_local(0, 0, 0, "minecraft:stone");
        assert!(!sec.is_empty());
        assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:stone");

        sec.set_local(0, 0, 0, "minecraft:air");
        assert!(sec.is_empty());
        assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:air");
    }

    #[test]
    fn test_extract_canonical_state() {
        assert_eq!(extract_canonical_state_str(""), "minecraft:air");
        assert_eq!(
            extract_canonical_state_str("minecraft:cave_air"),
            "minecraft:air"
        );
        assert_eq!(
            extract_canonical_state_str("minecraft:stone"),
            "minecraft:stone"
        );
        assert_eq!(
            extract_canonical_state_str("{\"state\":\"minecraft:dirt\"}"),
            "minecraft:dirt"
        );
    }

    #[test]
    fn test_padded_array_construction() {
        let mut sec = SectionStorage::new(IVec3::new(1, 2, 3));
        sec.set_local(0, 0, 0, "minecraft:diamond_block");

        let padded = sec.build_padded_array(|lx, ly, lz| {
            if lx == -1 && ly == 0 && lz == 0 {
                "minecraft:gold_block"
            } else {
                "minecraft:air"
            }
        });

        assert_eq!(padded.get_padded_state(1, 1, 1), "minecraft:diamond_block");
        assert_eq!(padded.get_padded_state(0, 1, 1), "minecraft:gold_block");
        assert_eq!(padded.get_padded_state(2, 2, 2), "minecraft:air");
    }
}
