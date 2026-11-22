//! # Mesh Vertex Spatial Welding
//!
//! Provides spatial vertex deduplication and index remapping for general 3D meshes.

use super::{MeshData, UvLayout};

impl MeshData {
    /// Welds co-located vertices within `tolerance` distance into shared topology.
    ///
    /// The `uv_layout` field selects the explicit policy:
    /// - `UvLayout::PerLoop`: only positions/normals are welded; per-loop UVs, colors
    ///   and secondary UVs stay attached to their loops, preserving UV seams.
    /// - `UvLayout::PerVertex` / `UvLayout::None`: per-vertex attributes are welded
    ///   together, refusing to merge vertices whose primary UVs differ.
    pub fn weld_vertices(&mut self, tolerance: f32) {
        if self.positions.is_empty() || tolerance <= 0.0 {
            return;
        }

        let per_loop = self.uv_layout == UvLayout::PerLoop;
        let inv_dist = 1.0 / tolerance;
        let mut coord_map = rustc_hash::FxHashMap::<[i32; 3], u32>::with_capacity_and_hasher(
            self.positions.len(),
            Default::default(),
        );
        let mut remap: Vec<u32> = Vec::with_capacity(self.positions.len());
        let mut new_positions: Vec<[f32; 3]> = Vec::with_capacity(self.positions.len());
        let mut new_normals: Vec<[f32; 3]> = Vec::with_capacity(self.normals.len());
        let mut new_uvs: Vec<[f32; 2]> = Vec::with_capacity(self.uvs.len());
        let mut new_sec_uvs: Option<Vec<[f32; 2]>> =
            self.secondary_uvs.as_ref().map(|_| Vec::new());
        let mut new_colors: Option<Vec<[f32; 4]>> = self.colors.as_ref().map(|_| Vec::new());

        for i in 0..self.positions.len() {
            let p = self.positions[i];
            let key = [
                (p[0] * inv_dist).round() as i32,
                (p[1] * inv_dist).round() as i32,
                (p[2] * inv_dist).round() as i32,
            ];

            if let Some(&existing_idx) = coord_map.get(&key) {
                let uv_matches = per_loop || self.uvs.is_empty() || {
                    let existing_uv = self.uvs.get(existing_idx as usize);
                    let current_uv = self.uvs.get(i);
                    match (existing_uv, current_uv) {
                        (Some(a), Some(b)) => (a[0] - b[0]).abs().max((a[1] - b[1]).abs()) < 1e-4,
                        (None, None) => true,
                        _ => false,
                    }
                };
                if uv_matches {
                    remap.push(existing_idx);
                    continue;
                }
            }

            let new_idx = new_positions.len() as u32;
            coord_map.insert(key, new_idx);
            remap.push(new_idx);
            new_positions.push(p);
            new_normals.push(self.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]));

            if !per_loop {
                if let Some(uv) = self.uvs.get(i) {
                    new_uvs.push(*uv);
                }
                if let (Some(sec_in), Some(sec_out)) = (&self.secondary_uvs, &mut new_sec_uvs) {
                    sec_out.push(sec_in.get(i).copied().unwrap_or([0.0, 0.0]));
                }
                if let (Some(col_in), Some(col_out)) = (&self.colors, &mut new_colors) {
                    col_out.push(col_in.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]));
                }
            }
        }

        if new_positions.len() == self.positions.len() {
            return;
        }

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            crate::threading::install(|| {
                self.indices.par_iter_mut().for_each(|idx| {
                    if let Some(&new_i) = remap.get(*idx as usize) {
                        *idx = new_i;
                    }
                });
                if let Some(ref mut quads) = self.quad_indices {
                    quads.par_iter_mut().for_each(|idx| {
                        if let Some(&new_i) = remap.get(*idx as usize) {
                            *idx = new_i;
                        }
                    });
                }
            });
        }

        #[cfg(not(feature = "parallel"))]
        {
            for idx in &mut self.indices {
                if let Some(&new_i) = remap.get(*idx as usize) {
                    *idx = new_i;
                }
            }
            if let Some(ref mut quads) = self.quad_indices {
                for idx in quads {
                    if let Some(&new_i) = remap.get(*idx as usize) {
                        *idx = new_i;
                    }
                }
            }
        }

        Self::resolve_face_index_collisions(
            &mut self.indices,
            self.quad_indices.as_deref_mut(),
            per_loop,
            &mut new_positions,
            &mut new_normals,
            &mut new_uvs,
            new_sec_uvs.as_mut(),
            new_colors.as_mut(),
        );

        self.positions = new_positions;
        self.normals = new_normals;
        if !per_loop {
            self.uvs = new_uvs;
            self.secondary_uvs = new_sec_uvs;
            self.colors = new_colors;
        }
    }

    /// Splits a single vertex when multiple non-degenerate faces collapse onto the same unordered
    /// vertex-index tuple during welding, preventing duplicate-face validation errors downstream.
    #[allow(clippy::too_many_arguments)]
    fn resolve_face_index_collisions(
        indices: &mut [u32],
        quad_indices: Option<&mut [u32]>,
        per_loop: bool,
        new_positions: &mut Vec<[f32; 3]>,
        new_normals: &mut Vec<[f32; 3]>,
        new_uvs: &mut Vec<[f32; 2]>,
        mut new_sec_uvs: Option<&mut Vec<[f32; 2]>>,
        mut new_colors: Option<&mut Vec<[f32; 4]>>,
    ) {
        let mut clone_vertex = |v0: u32| -> u32 {
            let fresh = new_positions.len() as u32;
            let src = v0 as usize;
            new_positions.push(new_positions[src]);
            if let Some(&n) = new_normals.get(src) {
                new_normals.push(n);
            }
            if !per_loop {
                if let Some(&uv) = new_uvs.get(src) {
                    new_uvs.push(uv);
                }
                if let Some(ref mut sec) = new_sec_uvs {
                    if let Some(&s) = sec.get(src) {
                        sec.push(s);
                    }
                }
                if let Some(ref mut cols) = new_colors {
                    if let Some(&c) = cols.get(src) {
                        cols.push(c);
                    }
                }
            }
            fresh
        };

        if let Some(quads) = quad_indices {
            let mut seen = rustc_hash::FxHashSet::<[u32; 4]>::default();
            let quad_count = quads.len() / 4;
            let sync_tris = indices.len() == quad_count * 6;
            for q in 0..quad_count {
                let q_base = q * 4;
                let mut key = [
                    quads[q_base],
                    quads[q_base + 1],
                    quads[q_base + 2],
                    quads[q_base + 3],
                ];
                key.sort_unstable();
                if key[0] == key[1] || key[1] == key[2] || key[2] == key[3] {
                    continue;
                }
                if !seen.insert(key) {
                    let v0 = quads[q_base];
                    let fresh = clone_vertex(v0);
                    quads[q_base] = fresh;
                    if sync_tris {
                        let tri_base = q * 6;
                        for slot in &mut indices[tri_base..tri_base + 6] {
                            if *slot == v0 {
                                *slot = fresh;
                            }
                        }
                    }
                }
            }
        } else {
            let mut seen = rustc_hash::FxHashSet::<[u32; 3]>::default();
            for tri in indices.as_chunks_mut::<3>().0 {
                let mut key = [tri[0], tri[1], tri[2]];
                key.sort_unstable();
                if key[0] == key[1] || key[1] == key[2] {
                    continue;
                }
                if !seen.insert(key) {
                    let fresh = clone_vertex(tri[0]);
                    tri[0] = fresh;
                }
            }
        }
    }
}
