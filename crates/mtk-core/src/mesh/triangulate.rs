//! Quad-to-triangle topology normalization for mixed polygon merging.

use crate::attributes::{AttributeData, AttributeDomain};

use super::{MeshData, UvLayout};

#[inline]
fn quad_tri_corner_map(quad: &[u32; 4], tris: &[u32; 6]) -> [usize; 6] {
    if tris[0] == quad[0] && tris[1] == quad[1] && tris[2] == quad[2] {
        return [0, 1, 2, 0, 2, 3];
    }
    if tris[0] == quad[1] && tris[1] == quad[2] && tris[2] == quad[3] {
        return [1, 2, 3, 0, 1, 3];
    }
    let mut map = [0usize; 6];
    for k in 0..6 {
        let target = tris[k];
        map[k] = quad.iter().position(|&v| v == target).unwrap_or(k % 4);
    }
    map
}

#[inline]
fn expand_vec_face<T: Copy>(src: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(src.len() * 2);
    for &item in src {
        out.push(item);
        out.push(item);
    }
    out
}

#[inline]
fn expand_vec_corner<T: Copy>(src: &[T], corner_maps: &[[usize; 6]]) -> Vec<T> {
    let mut out = Vec::with_capacity(corner_maps.len() * 6);
    for (q, map) in corner_maps.iter().enumerate() {
        let base = q * 4;
        for &c in map {
            out.push(src[base + c]);
        }
    }
    out
}

fn expand_face_attribute(data: &mut AttributeData, expected_quads: usize) {
    if data.len() != expected_quads {
        return;
    }
    match data {
        AttributeData::Float(v) => *v = expand_vec_face(v),
        AttributeData::Float2(v) => *v = expand_vec_face(v),
        AttributeData::Float3(v) => *v = expand_vec_face(v),
        AttributeData::Float4(v) => *v = expand_vec_face(v),
        AttributeData::Int8(v) => *v = expand_vec_face(v),
        AttributeData::Int16(v) => *v = expand_vec_face(v),
        AttributeData::Int32(v) => *v = expand_vec_face(v),
        AttributeData::UInt8(v) => *v = expand_vec_face(v),
        AttributeData::UInt16(v) => *v = expand_vec_face(v),
        AttributeData::UInt32(v) => *v = expand_vec_face(v),
        AttributeData::Bool(v) => *v = expand_vec_face(v),
        AttributeData::String(v) => {
            let mut out = Vec::with_capacity(v.len() * 2);
            for s in v.iter() {
                out.push(s.clone());
                out.push(s.clone());
            }
            *v = out;
        }
    }
}

fn expand_corner_attribute(data: &mut AttributeData, corner_maps: &[[usize; 6]]) {
    let expected_corners = corner_maps.len() * 4;
    if data.len() != expected_corners {
        return;
    }
    match data {
        AttributeData::Float(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Float2(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Float3(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Float4(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Int8(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Int16(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Int32(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::UInt8(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::UInt16(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::UInt32(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::Bool(v) => *v = expand_vec_corner(v, corner_maps),
        AttributeData::String(v) => {
            let mut out = Vec::with_capacity(corner_maps.len() * 6);
            for (q, map) in corner_maps.iter().enumerate() {
                let base = q * 4;
                for &c in map {
                    out.push(v[base + c].clone());
                }
            }
            *v = out;
        }
    }
}

impl MeshData {
    /// Converts a quad-topology mesh in place to pure triangle topology so every triangle owns
    /// explicit face and corner-loop attributes (`quad_indices` becomes `None`).
    ///
    /// No-op if the mesh is already non-quad topology (aside from clearing empty `quad_indices`).
    pub fn convert_to_triangles(&mut self) {
        if !self.is_quad_topology() {
            return;
        }
        let quad_count = self.quad_count();
        if quad_count == 0 || self.indices.len() < quad_count * 6 {
            self.quad_indices = None;
            return;
        }

        let quads = self.reconstruct_quad_indices();
        let mut corner_maps = Vec::with_capacity(quad_count);
        for q in 0..quad_count {
            let q_base = q * 4;
            let t_base = q * 6;
            let quad = [
                quads[q_base],
                quads[q_base + 1],
                quads[q_base + 2],
                quads[q_base + 3],
            ];
            let tris = [
                self.indices[t_base],
                self.indices[t_base + 1],
                self.indices[t_base + 2],
                self.indices[t_base + 3],
                self.indices[t_base + 4],
                self.indices[t_base + 5],
            ];
            corner_maps.push(quad_tri_corner_map(&quad, &tris));
        }

        if self.face_materials.len() == quad_count {
            self.face_materials = expand_vec_face(&self.face_materials);
        }
        if self.face_tint_indices.len() == quad_count {
            self.face_tint_indices = expand_vec_face(&self.face_tint_indices);
        }

        let expected_quad_loops = quad_count * 4;
        if self.uv_layout == UvLayout::PerLoop && self.uvs.len() == expected_quad_loops {
            self.uvs = expand_vec_corner(&self.uvs, &corner_maps);
        }
        if let Some(ref mut sec_uvs) = self.secondary_uvs {
            if sec_uvs.len() == expected_quad_loops {
                *sec_uvs = expand_vec_corner(sec_uvs, &corner_maps);
            }
        }
        if let Some(ref mut colors) = self.colors {
            if colors.len() == expected_quad_loops {
                *colors = expand_vec_corner(colors, &corner_maps);
            }
        }

        for attr in self.custom_attributes.values_mut() {
            match attr.domain {
                AttributeDomain::Face => expand_face_attribute(&mut attr.data, quad_count),
                AttributeDomain::Corner => expand_corner_attribute(&mut attr.data, &corner_maps),
                AttributeDomain::Point | AttributeDomain::Mesh => {}
            }
        }

        self.quad_indices = None;
    }

    /// Returns a pure-triangle copy of this mesh.
    pub fn to_triangles(&self) -> Self {
        let mut out = self.clone();
        out.convert_to_triangles();
        out
    }
}
