#![forbid(unsafe_code)]
pub mod baker;
pub mod builtin;
pub mod entity;
pub mod error;
pub mod parser;

// Top-level re-exports
pub use baker::{
    apply_uvlock_to_uvs, bake_face_exact, calculate_facing, default_face_uv,
    get_face_canonical_vertex, get_face_uvlock_transform, is_block_emissive, recalculate_winding,
    rotate_direction, rotate_element_point, rotate_point, BakedElement, BakedFace,
    BakedFaceGeometry, BakedModel, BakedModelDatabase, BakedVariantGroup, ModelBaker,
    ModelMeshOptions,
};
pub use entity::{BlockEntityData, EntityCategory};
pub use error::ModelError;
pub use parser::{
    is_air_like, mesh_to_obj_string, normalize_canonical_blockstate, BakedObjFace, BlockModelJson,
    BlockState, BlockStateDefinition, BlockStateResolver, BuiltinTransform, ElementJson, FaceJson,
    ModObjLoader, MultipartCondition, MultipartRule, ObjRawFace, ResolvedBlockModel,
    ResolvedElement, ResolvedFace, RotationJson, TextureValue, VariantEntry, VariantMatch,
    VariantModel, WavefrontObjParser,
};
