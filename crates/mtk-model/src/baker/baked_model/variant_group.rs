//! # Pre-baked Model Variant Groups
//!
//! Encapsulates discrete collections of pre-baked `BakedModel` variants (e.g. 0°, 90°, 180°, 270°
//! rotated rotations for dirt, stone, sand, lily pads) and provides 1:1 deterministic seed selection.

use mtk_core::{mc_coordinate_seed, JavaRandom};
use serde::{Deserialize, Serialize};

use super::model::BakedModel;

/// A collection of pre-baked model variants for a BlockState with relative selection weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedVariantGroup {
    /// Canonical BlockState identifier string, e.g. `"minecraft:dirt"`.
    pub base_state: String,
    /// Pre-baked models for each discrete variant.
    pub models: Vec<BakedModel>,
    /// Relative selection weights (defaults to 1 per variant in vanilla).
    pub weights: Vec<u32>,
    /// Pre-computed sum of all weights for fast modulo/LCG sampling.
    pub total_weight: u32,
}

impl BakedVariantGroup {
    /// Creates a new variant group with the specified models and weights.
    pub fn new(base_state: String, models: Vec<BakedModel>, weights: Vec<u32>) -> Self {
        assert_eq!(
            models.len(),
            weights.len(),
            "Model count must match weight count in BakedVariantGroup"
        );
        let total_weight = weights.iter().sum::<u32>().max(1);
        Self {
            base_state,
            models,
            weights,
            total_weight,
        }
    }

    /// Creates a trivial single-model variant group (weight = 1).
    pub fn single(base_state: String, model: BakedModel) -> Self {
        Self {
            base_state,
            models: vec![model],
            weights: vec![1],
            total_weight: 1,
        }
    }

    /// Returns the number of discrete variants in this group.
    #[inline]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Returns whether this group contains no models.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Returns the primary/default model variant (highest weight, fallback to first).
    #[inline]
    pub fn select_primary(&self) -> &BakedModel {
        if self.models.is_empty() {
            panic!("Cannot select_primary from empty BakedVariantGroup");
        }
        let mut best_idx = 0;
        let mut best_weight = self.weights[0];
        for (i, &w) in self.weights.iter().enumerate().skip(1) {
            if w > best_weight {
                best_weight = w;
                best_idx = i;
            }
        }
        &self.models[best_idx]
    }

    /// Selects a model variant index deterministically using a signed 64-bit coordinate seed (1:1 with Java LCG).
    pub fn select_index_by_seed(&self, seed: i64) -> usize {
        if self.models.len() <= 1 {
            return 0;
        }

        let mut rng = JavaRandom::new(seed);
        let mut target = rng.next_int(self.total_weight);

        for (i, &weight) in self.weights.iter().enumerate() {
            if target < weight {
                return i;
            }
            target = target.saturating_sub(weight);
        }

        0
    }

    /// Selects a model variant index deterministically using 3D world integer coordinates.
    #[inline]
    pub fn select_index_by_pos(&self, x: i32, y: i32, z: i32) -> usize {
        let seed = mc_coordinate_seed(x, y, z);
        self.select_index_by_seed(seed)
    }

    /// Selects a model variant deterministically using a signed 64-bit coordinate seed (1:1 with Java LCG).
    #[inline]
    pub fn select_by_seed(&self, seed: i64) -> &BakedModel {
        &self.models[self.select_index_by_seed(seed)]
    }

    /// Selects a model variant deterministically using 3D world integer coordinates.
    #[inline]
    pub fn select_by_pos(&self, x: i32, y: i32, z: i32) -> &BakedModel {
        let seed = mc_coordinate_seed(x, y, z);
        self.select_by_seed(seed)
    }

    /// Eliminates overlapping, duplicate, and interior coplanar contacting faces across all models in this group.
    pub fn deduplicate_faces(&mut self) -> usize {
        let mut removed = 0;
        for model in &mut self.models {
            removed += model.deduplicate_faces();
        }
        removed
    }

    /// Remaps all baked models in this group to atlas coordinates using a lookup closure.
    pub fn remap_to_atlas_with<F>(&mut self, mut lookup_fn: F)
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        for model in &mut self.models {
            model.remap_to_atlas_with(&mut lookup_fn);
        }
    }

    /// Re-evaluates opacity and culling metadata for all model variants against texture opacity.
    pub fn update_cull_meta_with_atlas<F>(&mut self, mut is_texture_opaque_fn: F)
    where
        F: FnMut(&str) -> Option<bool>,
    {
        for model in &mut self.models {
            model.update_cull_meta_with_atlas(&mut is_texture_opaque_fn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_variant_group() {
        let model = BakedModel {
            block_state: "minecraft:stone_bricks".to_string(),
            ..Default::default()
        };
        let group = BakedVariantGroup::single("minecraft:stone_bricks".to_string(), model);
        assert_eq!(group.len(), 1);
        assert_eq!(group.select_primary().block_state, "minecraft:stone_bricks");
        assert_eq!(
            group.select_by_pos(10, 20, 30).block_state,
            "minecraft:stone_bricks"
        );
    }

    #[test]
    fn test_multi_variant_distribution() {
        let mut models = Vec::new();
        for i in 0..4 {
            models.push(BakedModel {
                block_state: format!("minecraft:dirt#variant_{}", i),
                ..Default::default()
            });
        }
        let weights = vec![1, 1, 1, 1];
        let group = BakedVariantGroup::new("minecraft:dirt".to_string(), models, weights);

        let mut hits = [0usize; 4];
        for x in 0..100 {
            for z in 0..100 {
                let m = group.select_by_pos(x, 64, z);
                let idx: usize = m
                    .block_state
                    .split('_')
                    .next_back()
                    .unwrap()
                    .parse()
                    .unwrap();
                hits[idx] += 1;
            }
        }

        // Each variant should be hit roughly equally (~2500 times out of 10000)
        for &h in &hits {
            assert!(h > 2000 && h < 3000, "Unexpected hit count: {}", h);
        }
    }
}
