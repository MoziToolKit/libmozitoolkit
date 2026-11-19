//! # 3D Quart Biome Storage
//!
//! Provides dense, paletted 3D 4x4x4 quart biome storage for 16x16x16 chunk sections,
//! aligned with Minecraft 1.18+ (Caves & Cliffs) section data format.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// 3D 4x4x4 quart paletted biome storage for a 16x16x16 chunk section.
///
/// Contains 64 total quart cells, each representing a 4x4x4 block region.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SectionBiomes {
    /// Biome resource identifiers (e.g. `["minecraft:taiga", "minecraft:lush_caves"]`).
    pub palette: Vec<String>,
    /// 64 palette indices mapping each 4x4x4 block quart `(qy << 4) | (qz << 2) | qx`.
    /// When `palette.len() <= 1`, this is empty (all quarts implicitly index 0).
    pub indices: Vec<u8>,
    /// Bitmask of quarts explicitly assigned via `set_biome` or `new`.
    #[cfg_attr(feature = "serde", serde(default))]
    explicit_mask: u64,
}

impl SectionBiomes {
    /// Creates a new `SectionBiomes` instance.
    ///
    /// The supplied `indices` are treated as authoritative: every covered quart is marked
    /// as explicitly assigned, so vertical-variation checks may rely on them.
    pub fn new(palette: Vec<String>, indices: Vec<u8>) -> Self {
        let explicit_mask = match indices.len() {
            0 => 0,
            n if n >= 64 => u64::MAX,
            n => (1u64 << n) - 1,
        };
        Self {
            palette,
            indices,
            explicit_mask,
        }
    }

    /// Creates a dense 64-quart `SectionBiomes` whose cells are all initialised to
    /// `palette[0]` but marked as **unassigned**.
    ///
    /// This is the correct constructor for streaming providers that only cover part of a
    /// section (e.g. a selection-anchored live-sync quart grid). Rely on
    /// [`Self::set_sampled_quart_biome`] to write the real cells and then
    /// [`Self::fill_unwritten_nearest`] so placeholder values never leak into meshing.
    pub fn blank(palette: Vec<String>) -> Self {
        Self {
            palette,
            indices: vec![0u8; 64],
            explicit_mask: 0,
        }
    }

    /// Creates a uniform `SectionBiomes` with a single biome for all 64 quarts.
    pub fn uniform(biome: impl Into<String>) -> Self {
        Self {
            palette: vec![biome.into()],
            indices: Vec::new(),
            explicit_mask: 0,
        }
    }

    /// Returns the biome identifier for local section block coordinate `(lx, ly, lz)`.
    #[inline]
    pub fn get_biome(&self, lx: usize, ly: usize, lz: usize) -> Option<&str> {
        if self.palette.is_empty() {
            return None;
        }
        if self.palette.len() == 1 || self.indices.is_empty() {
            return Some(&self.palette[0]);
        }
        let qx = (lx >> 2) & 3;
        let qy = (ly >> 2) & 3;
        let qz = (lz >> 2) & 3;
        let idx = (qy << 4) | (qz << 2) | qx;
        if let Some(&pal_idx) = self.indices.get(idx) {
            self.palette
                .get(pal_idx as usize)
                .map(|s| s.as_str())
                .or_else(|| self.palette.first().map(|s| s.as_str()))
        } else {
            Some(&self.palette[0])
        }
    }

    /// Sets the biome identifier for local section block coordinate `(lx, ly, lz)`.
    pub fn set_biome(&mut self, lx: usize, ly: usize, lz: usize, biome: &str) {
        let qx = (lx >> 2) & 3;
        let qy = (ly >> 2) & 3;
        let qz = (lz >> 2) & 3;
        let idx = (qy << 4) | (qz << 2) | qx;
        self.explicit_mask |= 1u64 << idx;
        self.set_quart_biome(qx, qy, qz, biome);
        for other_qy in 0..4 {
            let other_idx = (other_qy << 4) | (qz << 2) | qx;
            if (self.explicit_mask & (1u64 << other_idx)) == 0 {
                self.set_quart_biome(qx, other_qy, qz, biome);
            }
        }
    }

    /// Sets the biome identifier for quart coordinate `(qx, qy, qz)` where each is in `0..4`.
    pub fn set_quart_biome(&mut self, qx: usize, qy: usize, qz: usize, biome: &str) {
        let idx = ((qy & 3) << 4) | ((qz & 3) << 2) | (qx & 3);
        if self.indices.is_empty() {
            self.indices = vec![0; 64];
            // A section carrying no dense index array was authoritatively uniform when
            // its palette had a single entry (e.g. `SectionBiomes::uniform` or a
            // single-biome NBT section), so every cell keeps that value. Multi-entry
            // palettes without data stay unassigned for the caller to fill.
            if self.palette.len() <= 1 {
                self.explicit_mask = u64::MAX;
            }
        }
        let pal_idx = if let Some(pos) = self.palette.iter().position(|p| p == biome) {
            pos as u8
        } else {
            self.palette.push(biome.to_string());
            (self.palette.len() - 1) as u8
        };
        if self.indices.len() == 64 {
            self.indices[idx] = pal_idx;
        }
    }

    /// Sets the biome for quart `(qx, qy, qz)` **and** marks it as explicitly assigned.
    ///
    /// Streaming providers must use this variant (instead of [`Self::set_quart_biome`]) so
    /// [`Self::fill_unwritten_nearest`] and [`Self::varies_vertically`] can distinguish
    /// real samples from placeholder cells.
    pub fn set_sampled_quart_biome(&mut self, qx: usize, qy: usize, qz: usize, biome: &str) {
        self.set_quart_biome(qx, qy, qz, biome);
        let idx = ((qy & 3) << 4) | ((qz & 3) << 2) | (qx & 3);
        if self.indices.len() == 64 {
            self.explicit_mask |= 1u64 << idx;
        }
    }

    /// Returns the number of quart cells that have been explicitly assigned.
    #[inline]
    pub fn explicit_count(&self) -> u32 {
        self.explicit_mask.count_ones()
    }

    /// Returns true when every quart cell carries an authoritative biome.
    ///
    /// A single-entry palette is authoritative by construction (every cell is that biome and
    /// `indices` is intentionally empty), which is what [`Self::uniform`] and the 2D save/sync
    /// paths rely on. Multi-entry sections are only fully assigned once all 64 cells were
    /// written through a sampled/dense constructor.
    #[inline]
    pub fn is_fully_assigned(&self) -> bool {
        if self.palette.is_empty() {
            return false;
        }
        self.palette.len() == 1 || (self.indices.len() == 64 && self.explicit_mask == u64::MAX)
    }

    /// Fills every unassigned quart cell with the biome of the nearest assigned cell
    /// (squared Euclidean distance in local quart space, ties broken by the lowest cell
    /// index). This keeps partially-covered sections self-consistent so placeholders can
    /// never leak into `varies_vertically`, the 5x5 biome kernel, or the mesh tint.
    pub fn fill_unwritten_nearest(&mut self) {
        if self.indices.len() != 64 || self.palette.is_empty() || self.explicit_mask == u64::MAX {
            return;
        }
        if self.explicit_mask == 0 {
            return;
        }
        let assigned: Vec<usize> = (0..64)
            .filter(|idx| (self.explicit_mask & (1u64 << idx)) != 0)
            .collect();
        for idx in 0..64usize {
            if (self.explicit_mask & (1u64 << idx)) != 0 {
                continue;
            }
            let qx = idx & 3;
            let qz = (idx >> 2) & 3;
            let qy = (idx >> 4) & 3;
            let mut best = assigned[0];
            let mut best_dist = i32::MAX;
            for &candidate in &assigned {
                let ax = candidate & 3;
                let az = (candidate >> 2) & 3;
                let ay = (candidate >> 4) & 3;
                let dist = (qx as i32 - ax as i32).pow(2)
                    + (qy as i32 - ay as i32).pow(2)
                    + (qz as i32 - az as i32).pow(2);
                if dist < best_dist {
                    best_dist = dist;
                    best = candidate;
                }
            }
            self.indices[idx] = self.indices[best];
            self.explicit_mask |= 1u64 << idx;
        }
    }

    /// Returns the biome identifier for quart coordinate `(qx, qy, qz)` where each is in `0..4`.
    #[inline]
    pub fn get_quart_biome(&self, qx: usize, qy: usize, qz: usize) -> Option<&str> {
        if self.palette.is_empty() {
            return None;
        }
        if self.palette.len() == 1 || self.indices.is_empty() {
            return Some(&self.palette[0]);
        }
        let idx = ((qy & 3) << 4) | ((qz & 3) << 2) | (qx & 3);
        if let Some(&pal_idx) = self.indices.get(idx) {
            self.palette
                .get(pal_idx as usize)
                .map(|s| s.as_str())
                .or_else(|| self.palette.first().map(|s| s.as_str()))
        } else {
            Some(&self.palette[0])
        }
    }

    /// Returns true if all 64 quarts share an identical biome.
    ///
    /// Only explicitly assigned quarts are considered; placeholder cells created via
    /// [`Self::blank`] never make a section look non-uniform.
    #[inline]
    pub fn is_uniform(&self) -> bool {
        if self.palette.len() <= 1 || self.indices.is_empty() {
            return true;
        }
        let mut first: Option<u8> = None;
        for (idx, &pal_idx) in self.indices.iter().enumerate().take(64) {
            if (self.explicit_mask & (1u64 << idx)) == 0 {
                continue;
            }
            match first {
                None => first = Some(pal_idx),
                Some(expected) if expected != pal_idx => return false,
                _ => {}
            }
        }
        true
    }

    /// Returns true if the biomes vary along the Y axis.
    ///
    /// Only explicitly assigned quarts are considered, so a partially-covered section
    /// with placeholder cells cannot flip the mesher into 3D (1024-column) mode by accident.
    pub fn varies_vertically(&self) -> bool {
        if self.palette.len() <= 1 || self.indices.is_empty() {
            return false;
        }
        for qx in 0..4 {
            for qz in 0..4 {
                let mut first: Option<&str> = None;
                for qy in 0..4 {
                    let idx = (qy << 4) | (qz << 2) | qx;
                    if (self.explicit_mask & (1u64 << idx)) == 0 {
                        continue;
                    }
                    let biome = self.get_quart_biome(qx, qy, qz);
                    match first {
                        None => first = biome,
                        Some(expected) if Some(expected) != biome => return true,
                        _ => {}
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Vec<String> {
        vec![
            "minecraft:desert".to_string(),
            "minecraft:plains".to_string(),
        ]
    }

    #[test]
    fn blank_cells_are_not_treated_as_authoritative_data() {
        let mut sb = SectionBiomes::blank(palette());
        assert_eq!(sb.explicit_count(), 0);
        assert!(sb.is_uniform());
        assert!(!sb.varies_vertically());

        // A single sampled column must not make the whole section look 3D.
        sb.set_sampled_quart_biome(0, 0, 0, "minecraft:plains");
        assert_eq!(sb.explicit_count(), 1);
        assert!(!sb.varies_vertically());
    }

    #[test]
    fn varies_vertically_ignores_placeholder_cells() {
        let mut sb = SectionBiomes::blank(palette());
        // Placeholder cells default to palette[0] (desert). Only explicit cells count.
        sb.set_sampled_quart_biome(0, 0, 0, "minecraft:desert");
        sb.set_sampled_quart_biome(0, 3, 0, "minecraft:plains");
        assert!(sb.varies_vertically());

        let mut uniform = SectionBiomes::blank(palette());
        uniform.set_sampled_quart_biome(0, 0, 0, "minecraft:desert");
        uniform.set_sampled_quart_biome(2, 1, 1, "minecraft:desert");
        assert!(!uniform.varies_vertically());
    }

    #[test]
    fn fill_unwritten_nearest_completes_partial_sections() {
        let mut sb = SectionBiomes::blank(palette());
        // Simulates a selection-anchored quart grid sampling only x=4 (qx=1) and x=8 (qx=2).
        sb.set_sampled_quart_biome(1, 0, 0, "minecraft:desert");
        sb.set_sampled_quart_biome(2, 0, 0, "minecraft:plains");
        assert!(!sb.is_fully_assigned());

        sb.fill_unwritten_nearest();
        assert!(sb.is_fully_assigned());
        assert_eq!(sb.get_quart_biome(0, 0, 0), Some("minecraft:desert"));
        assert_eq!(sb.get_quart_biome(1, 0, 0), Some("minecraft:desert"));
        assert_eq!(sb.get_quart_biome(2, 0, 0), Some("minecraft:plains"));
        assert_eq!(sb.get_quart_biome(3, 0, 0), Some("minecraft:plains"));
        // Nearest fill must never resurrect palette[0] where the nearest sample is plains.
        assert_eq!(sb.get_quart_biome(3, 3, 3), Some("minecraft:plains"));
        // Vertical layers inherit their own column, not palette[0].
        assert_eq!(sb.get_quart_biome(3, 0, 0), Some("minecraft:plains"));
    }

    #[test]
    fn new_marks_supplied_indices_authoritative() {
        // NBT / section-snapshot data covers all 64 quarts and must stay authoritative.
        let indices: Vec<u8> = (0..64).map(|i| if i < 32 { 0 } else { 1 }).collect();
        let sb = SectionBiomes::new(palette(), indices);
        assert!(sb.is_fully_assigned());
        assert!(sb.varies_vertically());
    }
}
