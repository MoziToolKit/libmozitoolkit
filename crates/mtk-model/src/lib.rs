#![forbid(unsafe_code)]

mod baker;
mod builtin;
mod entity;
mod error;
mod parser;

// Top-level re-exports
pub use baker::{
    is_block_emissive, is_known_non_cube, is_opaque_block, BakedElement, BakedFace, BakedModel,
    BakedModelDatabase, BakedVariantGroup, ModelBaker,
};
pub use builtin::BuiltinModelRegistry;
pub use entity::{
    banner, build_entity_render_plan, entity_render_key, painting, skull, ArmorStandData,
    BlockEntityData, EntityBaker, EntityInstance, EntityKindData, EntityMaterialRoute,
    EntityRenderConfig, EntityRenderNode, EntityRenderPlan, EntityRenderTransform,
    EntityRenderWarning, EntityValidationError, ItemFrameData, PaintingData,
    ENTITY_RENDER_SCHEMA_VERSION,
};
pub use error::ModelError;
pub use parser::{
    mesh_to_obj_string, normalize_canonical_blockstate, BakedObjFace, BlockModelJson, BlockState,
    BlockStateDefinition, BlockStateResolver, ResolvedBlockModel, VariantModel,
};
