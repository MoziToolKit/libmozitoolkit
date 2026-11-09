use glam::Vec2;

#[inline]
pub fn tuples_to_vec2(uvs: &[(f32, f32)]) -> Vec<Vec2> {
    uvs.iter().map(|(u, v)| Vec2::new(*u, *v)).collect()
}

#[inline]
pub fn vec2_to_tuples(vecs: &[Vec2]) -> Vec<(f32, f32)> {
    vecs.iter().map(|v| (v.x, v.y)).collect()
}
