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
    banner, build_entity_render_plan, entity_render_key, find_supported_visual_entity,
    host_transform_to_mc, is_supported_visual_entity, item_frame, painting, skull,
    supported_visual_entity_ids, ArmorStandData, BlockEntityData, EntityBaker,
    EntityBuildCapability, EntityInstance, EntityKindData, EntityMaterialRoute, EntityRenderConfig,
    EntityRenderNode, EntityRenderPlan, EntityRenderTransform, EntityRenderWarning,
    EntityValidationError, ItemFrameData, PaintingData, SupportedVisualEntity,
    ENTITY_ID_ARMOR_STAND, ENTITY_ID_GLOW_ITEM_FRAME, ENTITY_ID_ITEM_FRAME, ENTITY_ID_PAINTING,
    ENTITY_RENDER_SCHEMA_VERSION, SUPPORTED_VISUAL_ENTITIES,
};
pub use error::ModelError;
pub use parser::{
    mesh_to_obj_string, normalize_canonical_blockstate, BakedObjFace, BlockModelJson, BlockState,
    BlockStateDefinition, BlockStateResolver, ResolvedBlockModel, VariantModel,
};
