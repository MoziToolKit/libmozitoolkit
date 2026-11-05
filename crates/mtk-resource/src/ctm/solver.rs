use crate::ctm::algorithms::{
    compact::solve_compact_ctm_with_tangents,
    directional::{
        solve_horizontal_vertical_with_tangents, solve_horizontal_with_tangents,
        solve_top_with_tangents, solve_vertical_horizontal_with_tangents,
        solve_vertical_with_tangents,
    },
    full::solve_full_ctm_with_tangents,
    patterns::{solve_overlay_with_tangents, solve_random, solve_repeat_with_tangents},
};
use crate::ctm::types::{extract_block_name, ConnectLogic, CtmMethod, CtmRule};
use crate::identifier::{ResourceLocation, DEFAULT_NAMESPACE};
use glam::IVec3;
use mtk_core::Direction;
use std::collections::HashMap;

impl CtmRule {
    /// Solves the final CTM sub-tile ResourceLocation for this face with explicit 2D face tangents `(up, left)`.
    pub fn solve_tile_with_tangents<F, S>(
        &self,
        state: &str,
        face: Direction,
        tangents: (Direction, Direction),
        world_pos: IVec3,
        get_block: &F,
    ) -> Option<ResourceLocation>
    where
        F: Fn(IVec3) -> Option<S>,
        S: AsRef<str>,
    {
        let check_connect = |offset: IVec3| -> bool {
            let n_pos = world_pos + offset;
            if let Some(neighbor_state) = get_block(n_pos) {
                let n_str = neighbor_state.as_ref();
                match &self.connect_logic {
                    ConnectLogic::SameBlock => {
                        extract_block_name(state) == extract_block_name(n_str)
                    }
                    ConnectLogic::SameState => state == n_str,
                    ConnectLogic::BlockNames(names) => {
                        let n_name = extract_block_name(n_str);
                        let canonical = if n_name.contains(':') {
                            n_name.to_string()
                        } else {
                            format!("{}:{}", DEFAULT_NAMESPACE, n_name)
                        };
                        names.contains(&canonical)
                    }
                    ConnectLogic::SameTile | ConnectLogic::Textures(_) => {
                        extract_block_name(state) == extract_block_name(n_str)
                    }
                }
            } else {
                false
            }
        };

        match &self.method {
            CtmMethod::Full { inner_seams } => {
                let tile_idx =
                    solve_full_ctm_with_tangents(face, tangents, *inner_seams, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::Compact { .. } => {
                let tile_idx = solve_compact_ctm_with_tangents(face, tangents, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::Horizontal => {
                let tile_idx = solve_horizontal_with_tangents(face, tangents, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::Vertical => {
                let tile_idx = solve_vertical_with_tangents(face, tangents, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::HorizontalVertical => {
                let tile_idx =
                    solve_horizontal_vertical_with_tangents(face, tangents, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::VerticalHorizontal => {
                let tile_idx =
                    solve_vertical_horizontal_with_tangents(face, tangents, &check_connect);
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::Top => {
                if solve_top_with_tangents(face, tangents, &check_connect) {
                    self.tiles.first().and_then(|t| t.clone())
                } else {
                    None
                }
            }
            CtmMethod::Repeat { width, height } => {
                let tile_idx =
                    solve_repeat_with_tangents(face, tangents, world_pos, *width, *height)?;
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
            CtmMethod::Random {
                weights,
                total_weight,
                symmetry,
                linked,
            } => {
                if self.tiles.is_empty() {
                    return None;
                }
                let chosen_idx = solve_random(
                    state,
                    face,
                    world_pos,
                    weights,
                    *total_weight,
                    *symmetry,
                    *linked,
                    self.tiles.len(),
                    get_block,
                );
                self.tiles.get(chosen_idx).and_then(|t| t.clone())
            }
            CtmMethod::Fixed => self.tiles.first().and_then(|t| t.clone()),
            CtmMethod::Overlay => {
                let tile_idx = solve_overlay_with_tangents(face, tangents, &check_connect)?;
                self.tiles.get(tile_idx).and_then(|t| t.clone())
            }
        }
    }
}

/// High-performance CTM Rule Solver indexed for fast per-face resolution during chunk meshing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CtmSolver {
    pub rules: Vec<CtmRule>,
    pub rules_by_block: HashMap<String, Vec<usize>>,
    pub rules_by_tile: HashMap<ResourceLocation, Vec<usize>>,
}

impl CtmSolver {
    /// Creates a new `CtmSolver` from an array of loaded `CtmRule`s, sorting by priority.
    pub fn new(mut rules: Vec<CtmRule>) -> Self {
        rules.sort_by_key(|r| std::cmp::Reverse(r.priority));

        let mut rules_by_block: HashMap<String, Vec<usize>> = HashMap::new();
        let mut rules_by_tile: HashMap<ResourceLocation, Vec<usize>> = HashMap::new();

        for (idx, rule) in rules.iter().enumerate() {
            for mb in &rule.match_blocks {
                rules_by_block
                    .entry(mb.block.clone())
                    .or_default()
                    .push(idx);
            }
            for mt in &rule.match_tiles {
                rules_by_tile.entry(mt.clone()).or_default().push(idx);
            }
        }

        Self {
            rules,
            rules_by_block,
            rules_by_tile,
        }
    }

    /// Resolves the final CTM tile for a block face given its state, orientation, position, and neighbor query closure.
    /// Uses canonical face tangents.
    pub fn resolve_face<F, S>(
        &self,
        state: &str,
        face: Direction,
        world_pos: IVec3,
        base_tile: Option<&ResourceLocation>,
        biome: Option<&str>,
        get_block: F,
    ) -> Option<ResourceLocation>
    where
        F: Fn(IVec3) -> Option<S>,
        S: AsRef<str>,
    {
        self.resolve_face_with_tangents(
            state,
            face,
            face.canonical_face_tangents(),
            world_pos,
            base_tile,
            biome,
            get_block,
        )
    }

    /// Resolves the final CTM tile for a block face with explicit 2D face tangents `(up, left)`.
    pub fn resolve_face_with_tangents<F, S>(
        &self,
        state: &str,
        face: Direction,
        tangents: (Direction, Direction),
        world_pos: IVec3,
        base_tile: Option<&ResourceLocation>,
        biome: Option<&str>,
        get_block: F,
    ) -> Option<ResourceLocation>
    where
        F: Fn(IVec3) -> Option<S>,
        S: AsRef<str>,
    {
        if self.rules.is_empty() {
            return None;
        }

        let block_name = extract_block_name(state);
        let canonical_name = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("{}:{}", DEFAULT_NAMESPACE, block_name)
        };

        // 1. Try match by block
        if let Some(rule_indices) = self.rules_by_block.get(&canonical_name) {
            for &idx in rule_indices {
                let rule = &self.rules[idx];
                if rule.matches_block(state, face, world_pos, base_tile, biome) {
                    if let Some(tile) =
                        rule.solve_tile_with_tangents(state, face, tangents, world_pos, &get_block)
                    {
                        return Some(tile);
                    }
                }
            }
        }

        // 2. Try match by base tile
        if let Some(tile_loc) = base_tile {
            if let Some(rule_indices) = self.rules_by_tile.get(tile_loc) {
                for &idx in rule_indices {
                    let rule = &self.rules[idx];
                    if rule.matches_block(state, face, world_pos, base_tile, biome) {
                        if let Some(tile) = rule
                            .solve_tile_with_tangents(state, face, tangents, world_pos, &get_block)
                        {
                            return Some(tile);
                        }
                    }
                }
            }
        }

        None
    }

    /// Serializes internal rules list to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.rules)
    }

    /// Deserializes a `CtmSolver` from a JSON string of rules.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let rules: Vec<CtmRule> = serde_json::from_str(json)?;
        Ok(Self::new(rules))
    }
}
