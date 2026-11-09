//! Quad emission helpers shared by the mesh-level pixel splitter.

use crate::mesh::MeshData;

/// Accumulates output quads (indices, loop UVs, material, tint) before emission.
#[derive(Default)]
pub(crate) struct QuadAccumulator {
    pub(crate) indices: Vec<[u32; 4]>,
    pub(crate) uvs: Vec<[[f32; 2]; 4]>,
    pub(crate) materials: Vec<u16>,
    pub(crate) tints: Vec<i16>,
}

impl QuadAccumulator {
    pub(crate) fn push(&mut self, indices: [u32; 4], uvs: [[f32; 2]; 4], material: u16, tint: i16) {
        self.indices.push(indices);
        self.uvs.push(uvs);
        self.materials.push(material);
        self.tints.push(tint);
    }
}

/// Emits an existing quad unchanged, copying its vertices and interpolated attributes.
pub(crate) fn emit_quad_copy(
    out: &mut MeshData,
    quads: &mut QuadAccumulator,
    positions: [[f32; 3]; 4],
    normals: [[f32; 3]; 4],
    secondary: Option<[[f32; 2]; 4]>,
    colors: Option<[[f32; 4]; 4]>,
    uvs: [[f32; 2]; 4],
    material: u16,
    tint: i16,
) {
    let base = out.positions.len() as u32;
    out.positions.extend_from_slice(&positions);
    out.normals.extend_from_slice(&normals);
    if let Some(sec) = secondary {
        out.secondary_uvs
            .get_or_insert_with(Vec::new)
            .extend_from_slice(&sec);
    }
    if let Some(col) = colors {
        out.colors
            .get_or_insert_with(Vec::new)
            .extend_from_slice(&col);
    }
    quads.push([base, base + 1, base + 2, base + 3], uvs, material, tint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_quad_copy_boundary_none_secondary_and_colors() {
        let mut out = MeshData::new();
        let mut quads = QuadAccumulator::default();
        let positions = [[0.0; 3]; 4];
        let normals = [[0.0, 1.0, 0.0]; 4];
        let uvs = [[0.0, 0.0]; 4];

        // 1. Initial out has secondary_uvs = None, colors = None.
        // Calling emit_quad_copy with Some(secondary) should auto-initialize secondary_uvs.
        let sec = Some([[0.5, 0.5]; 4]);
        emit_quad_copy(
            &mut out, &mut quads, positions, normals, sec, None, uvs, 0, -1,
        );
        assert!(out.secondary_uvs.is_some());
        assert_eq!(out.secondary_uvs.as_ref().unwrap().len(), 4);
        assert!(out.colors.is_none());

        // 2. Calling with Some(colors) should auto-initialize colors.
        let col = Some([[1.0, 0.0, 0.0, 1.0]; 4]);
        emit_quad_copy(
            &mut out, &mut quads, positions, normals, None, col, uvs, 0, -1,
        );
        assert!(out.colors.is_some());
        assert_eq!(out.colors.as_ref().unwrap().len(), 4);

        // 3. Calling with None, None when both are already initialized.
        emit_quad_copy(
            &mut out, &mut quads, positions, normals, None, None, uvs, 0, -1,
        );
        assert_eq!(out.positions.len(), 12);
        assert_eq!(quads.indices.len(), 3);
    }
}
