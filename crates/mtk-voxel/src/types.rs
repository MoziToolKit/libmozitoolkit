use glam::Vec3;
use mtk_resource::CtmSolver;
use mtk_texture::AtlasAddressMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Coordinate space transformation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CoordinateSystem {
    /// Canonical Minecraft coordinate system: +X East, +Y Up, +Z South.
    #[default]
    Minecraft,
    /// Standard Z-up right-handed coordinate system: +X East, +Y North (-Z_mc), +Z Up (+Y_mc).
    /// Used by Unreal Engine, Blender, 3ds Max, etc.
    ZUpRightHanded,
    /// Standard Y-up right-handed coordinate system: +X East, +Y Up (+Y_mc), +Z South (+Z_mc).
    /// Matches the glTF / Godot / Three.js / WebGPU convention (+Z forward, camera looking down -Z).
    ///
    /// Note: Minecraft's native world space already satisfies this convention, so this variant is
    /// numerically identical to [`CoordinateSystem::Minecraft`]. It exists to make the target
    /// convention explicit at call sites. (Unity uses a left-handed Y-up space and needs an extra mirror.)
    YUpRightHanded,
}

/// Strategy for handling unknown blocks (blocks without models or textures in model_db/atlas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UnknownBlockStrategy {
    /// Emit a 6-faced unit cube with the mozi:fallback checkerboard texture (default).
    #[default]
    FallbackCube,
    /// Completely skip geometry for unknown blocks (treat as air).
    IgnoreAsAir,
}

/// Configuration options for the voxel section and world mesher.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MesherConfig {
    /// Target coordinate system transformation.
    pub coordinate_system: CoordinateSystem,
    /// Whether to center local geometry relative to active selection origin (bottom center).
    pub origin_centered: bool,
    /// Whether to weld adjacent coplanar/shared vertices into manifold topology.
    pub weld_vertices: bool,
    /// Optional selection bounds `(min_pos, size)`: `([min_x, min_y, min_z], [size_x, size_y, size_z])`.
    pub selection_bounds: Option<([i32; 3], [i32; 3])>,
    /// Whether to calculate 4-corner ambient occlusion on faces.
    pub enable_ao: bool,
    /// Whether internal overlapping faces of complex non-cube models are clipped.
    pub exclude_hidden_volume: bool,
    /// Whether to calculate fluid top and side surfaces.
    pub mesh_fluids: bool,
    /// Optional CTM rule solver for connected textures.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub ctm_solver: Option<Arc<CtmSolver>>,
    /// Optional Atlas address map for UV remapping and material slotting.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub atlas_address_map: Option<Arc<AtlasAddressMap>>,
    /// Optional BiomeResolver for foliage, grass, and water tinting.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub biome_resolver: Option<Arc<mtk_material::BiomeResolver>>,
    /// Optional resident vanilla colormaps for foliage, grass, and dry foliage sampling.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub colormaps: Option<Arc<mtk_material::VanillaColormaps>>,
    /// Optional custom material/texture alias map.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub custom_aliases: Option<Arc<std::collections::HashMap<String, Vec<String>>>>,
    /// Whether to enable vanilla alternate block models (deterministic random rotation for dirt, stone, sand, etc.).
    pub enable_alternate_blocks: bool,
    /// Whether to enable deterministic position offsets for plants (flowers, tall grass, roots, etc.).
    pub enable_random_offsets: bool,
    /// Whether to route animated textures (water, lava, fire, etc.) to dedicated animated atlas chunks.
    pub enable_animated_textures: bool,
    /// Optional procedural banner baker for multi-layer composite banners.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub banner_baker: Option<Arc<mtk_texture::BannerBaker>>,
    /// Strategy for handling unknown blocks lacking both models and textures.
    pub unknown_block_strategy: UnknownBlockStrategy,
    /// Whether to enable smooth biome blending across quad face corners.
    pub smooth_biome_blend: bool,
}

impl Default for MesherConfig {
    fn default() -> Self {
        Self {
            coordinate_system: CoordinateSystem::Minecraft,
            origin_centered: true,
            weld_vertices: true,
            selection_bounds: None,
            enable_ao: true,
            exclude_hidden_volume: true,
            mesh_fluids: true,
            ctm_solver: None,
            atlas_address_map: None,
            biome_resolver: None,
            colormaps: None,
            custom_aliases: None,
            enable_alternate_blocks: true,
            enable_random_offsets: true,
            enable_animated_textures: true,
            banner_baker: None,
            unknown_block_strategy: UnknownBlockStrategy::FallbackCube,
            smooth_biome_blend: true,
        }
    }
}

impl MesherConfig {
    /// Converts a 3D vertex position in Minecraft coordinates `(x, y, z)`
    /// to the configured target coordinate system, taking into account origin centering.
    #[inline]
    pub fn transform_position(&self, pos: Vec3) -> Vec3 {
        let centered = if self.origin_centered {
            if let Some((min, size)) = self.selection_bounds {
                let cx = min[0] as f32 + size[0] as f32 / 2.0;
                let by = min[1] as f32;
                let cz = min[2] as f32 + size[2] as f32 / 2.0;
                Vec3::new(pos.x - cx, pos.y - by, pos.z - cz)
            } else {
                pos
            }
        } else {
            pos
        };

        match self.coordinate_system {
            CoordinateSystem::Minecraft => centered,
            CoordinateSystem::ZUpRightHanded => {
                mtk_core::mc_world_to_z_up(centered.x, centered.y, centered.z)
            }
            CoordinateSystem::YUpRightHanded => centered,
        }
    }

    /// Converts a direction vector (normal) to the configured target coordinate system.
    /// Direction vectors are rotated/axis-swapped, but never translated by origin.
    #[inline]
    pub fn transform_direction(&self, dir: Vec3) -> Vec3 {
        match self.coordinate_system {
            CoordinateSystem::Minecraft => dir,
            CoordinateSystem::ZUpRightHanded => mtk_core::mc_world_to_z_up(dir.x, dir.y, dir.z),
            CoordinateSystem::YUpRightHanded => dir,
        }
    }
}

/// Errors that can occur during voxel storage or meshing operations.
#[derive(Debug, Error)]
pub enum VoxelError {
    #[error("Thread pool error: {0}")]
    ThreadPoolError(String),
    #[error("Coordinate out of bounds: ({0}, {1}, {2})")]
    OutOfBounds(i32, i32, i32),
    #[error("Malformed snapshot data: {0}")]
    MalformedSnapshot(String),
    #[error("Invalid point cloud data: {0}")]
    InvalidPointCloud(String),
    #[error("Missing vanilla base pack: {0}")]
    MissingVanillaBasePack(String),
    #[error("Missing assets: {0}")]
    MissingAssets(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(system: CoordinateSystem) -> MesherConfig {
        MesherConfig {
            coordinate_system: system,
            origin_centered: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_y_up_is_identity() {
        let c = cfg(CoordinateSystem::YUpRightHanded);
        let p = c.transform_position(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(p, Vec3::new(2.0, 3.0, 4.0));
        let d = c.transform_direction(Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(d, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_z_up_maps_mc_to_east_north_up() {
        let c = cfg(CoordinateSystem::ZUpRightHanded);
        // MC +Z (South) becomes -Y (North); MC +Y (Up) becomes +Z (Up).
        assert_eq!(
            c.transform_position(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(1.0, -3.0, 2.0)
        );
    }

    /// Every supported output basis must remain right-handed (determinant +1) so that
    /// face winding and normals are never mirrored.
    #[test]
    fn test_all_coordinate_systems_preserve_handedness() {
        for system in [
            CoordinateSystem::Minecraft,
            CoordinateSystem::ZUpRightHanded,
            CoordinateSystem::YUpRightHanded,
        ] {
            let c = cfg(system);
            let ex = c.transform_direction(Vec3::X);
            let ey = c.transform_direction(Vec3::Y);
            let ez = c.transform_direction(Vec3::Z);
            // In a right-handed basis, X × Y == Z.
            let cross = ex.cross(ey);
            assert!(
                cross.abs_diff_eq(ez, 1e-6),
                "Coordinate system {:?} is not right-handed: X×Y={:?}, Z={:?}",
                system,
                cross,
                ez
            );
        }
    }

    #[test]
    fn test_origin_centered_uses_selection_bounds() {
        let mut c = cfg(CoordinateSystem::Minecraft);
        c.origin_centered = true;
        c.selection_bounds = Some(([0, 64, 0], [16, 16, 16]));
        // Block center offset: (x - 8, y - 64, z - 8).
        assert_eq!(
            c.transform_position(Vec3::new(8.0, 70.0, 8.0)),
            Vec3::new(0.0, 6.0, 0.0)
        );
    }
}
