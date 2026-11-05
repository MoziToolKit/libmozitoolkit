pub(crate) mod height;
pub(crate) mod mesher;
pub(crate) mod uv;

pub use height::FluidType;
pub(crate) use mesher::emit_fluid_geometry;
pub use uv::{
    batch_repair_fluid_uv, get_fluid_side_uvs, get_fluid_side_uvs_vec2, get_fluid_top_uvs,
    get_fluid_top_uvs_vec2, repair_mesh_fluid_uv_inplace, repair_quad_fluid_uv,
};
