//! MesherConfig Python bindings.

use crate::material::{PyBiomeResolver, PyVanillaColormaps};
use crate::texture::PyBakedAtlas;
use mtk_voxel::{CoordinateSystem, MesherConfig};
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Mesher generation configuration.
#[pyclass(name = "MesherConfig")]
#[derive(Debug, Clone, Default)]
pub struct PyMesherConfig {
    pub(crate) inner: MesherConfig,
    pub(crate) num_threads: Option<usize>,
}

#[pymethods]
impl PyMesherConfig {
    #[new]
    #[pyo3(signature = (enable_ao=true, mesh_fluids=true, z_up_coordinates=true, origin_centered=true, weld_vertices=true, num_threads=None, atlas=None, biome_resolver=None, custom_aliases=None, enable_alternate_blocks=true, enable_random_offsets=true, ctm_solver=None, enable_animated_textures=true, colormaps=None, unknown_block_strategy=None, smooth_biome_blend=true))]
    pub fn new(
        enable_ao: bool,
        mesh_fluids: bool,
        z_up_coordinates: bool,
        origin_centered: bool,
        weld_vertices: bool,
        num_threads: Option<usize>,
        atlas: Option<&PyBakedAtlas>,
        biome_resolver: Option<&PyBiomeResolver>,
        custom_aliases: Option<HashMap<String, Vec<String>>>,
        enable_alternate_blocks: bool,
        enable_random_offsets: bool,
        ctm_solver: Option<&crate::ctm::PyCtmSolver>,
        enable_animated_textures: bool,
        colormaps: Option<&PyVanillaColormaps>,
        unknown_block_strategy: Option<&str>,
        smooth_biome_blend: bool,
    ) -> Self {
        let strategy = match unknown_block_strategy {
            Some("IGNORE_AS_AIR") | Some("ignore_as_air") | Some("air") => {
                mtk_voxel::UnknownBlockStrategy::IgnoreAsAir
            }
            _ => mtk_voxel::UnknownBlockStrategy::FallbackCube,
        };
        let config = MesherConfig {
            enable_ao,
            mesh_fluids,
            origin_centered,
            weld_vertices,
            coordinate_system: if z_up_coordinates {
                CoordinateSystem::ZUpRightHanded
            } else {
                CoordinateSystem::Minecraft
            },
            atlas_address_map: atlas.map(|a| Arc::new(a.inner.address_map.clone())),
            biome_resolver: biome_resolver.map(|r| Arc::new(r.inner.clone())),
            custom_aliases: custom_aliases.map(Arc::new),
            enable_alternate_blocks,
            enable_random_offsets,
            ctm_solver: ctm_solver.map(|s| s.inner.clone()),
            enable_animated_textures,
            banner_baker: atlas.and_then(|a| a.banner_baker.clone()),
            colormaps: colormaps.map(|c| c.inner.clone()),
            unknown_block_strategy: strategy,
            smooth_biome_blend,
            ..Default::default()
        };
        Self {
            inner: config,
            num_threads,
        }
    }

    /// Sets or clears colormaps.
    pub fn set_colormaps(&mut self, colormaps: Option<&PyVanillaColormaps>) {
        self.inner.colormaps = colormaps.map(|c| c.inner.clone());
    }

    /// Sets or clears the CTM solver for connected textures.
    pub fn set_ctm_solver(&mut self, solver: Option<&crate::ctm::PyCtmSolver>) {
        self.inner.ctm_solver = solver.map(|s| s.inner.clone());
    }

    #[getter]
    pub fn enable_ao(&self) -> bool {
        self.inner.enable_ao
    }

    #[getter]
    pub fn mesh_fluids(&self) -> bool {
        self.inner.mesh_fluids
    }

    #[getter]
    pub fn origin_centered(&self) -> bool {
        self.inner.origin_centered
    }

    #[getter]
    pub fn weld_vertices(&self) -> bool {
        self.inner.weld_vertices
    }

    #[getter]
    pub fn enable_animated_textures(&self) -> bool {
        self.inner.enable_animated_textures
    }

    #[getter]
    pub fn z_up_coordinates(&self) -> bool {
        matches!(
            self.inner.coordinate_system,
            CoordinateSystem::ZUpRightHanded
        )
    }

    #[getter]
    pub fn unknown_block_strategy(&self) -> &'static str {
        match self.inner.unknown_block_strategy {
            mtk_voxel::UnknownBlockStrategy::FallbackCube => "FALLBACK_CUBE",
            mtk_voxel::UnknownBlockStrategy::IgnoreAsAir => "IGNORE_AS_AIR",
        }
    }

    #[setter]
    pub fn set_unknown_block_strategy(&mut self, strategy: &str) {
        self.inner.unknown_block_strategy = match strategy {
            "IGNORE_AS_AIR" | "ignore_as_air" | "air" => {
                mtk_voxel::UnknownBlockStrategy::IgnoreAsAir
            }
            _ => mtk_voxel::UnknownBlockStrategy::FallbackCube,
        };
    }

    #[getter]
    pub fn smooth_biome_blend(&self) -> bool {
        self.inner.smooth_biome_blend
    }

    #[setter]
    pub fn set_smooth_biome_blend(&mut self, enabled: bool) {
        self.inner.smooth_biome_blend = enabled;
    }

    #[getter]
    pub fn num_threads(&self) -> Option<usize> {
        self.num_threads
    }

    fn __repr__(&self) -> String {
        format!(
            "<MesherConfig ao={} fluids={} centered={} weld={} ctm={} coord={:?} threads={:?}>",
            self.inner.enable_ao,
            self.inner.mesh_fluids,
            self.inner.origin_centered,
            self.inner.weld_vertices,
            self.inner.ctm_solver.is_some(),
            self.inner.coordinate_system,
            self.num_threads
        )
    }
}
