#![forbid(unsafe_code)]

mod baker;
mod block_entity;
mod builtin;
mod error;
mod item;
mod parser;

// Top-level re-exports
pub use baker::{
    is_block_emissive, is_known_non_cube, is_opaque_block, BakedElement, BakedFace, BakedModel,
    BakedModelDatabase, BakedVariantGroup, ModelBakeFailure, ModelBaker,
};
pub use block_entity::{
    banner, skull, BannerPatternLayer, BlockEntityData, PlayerProfileData, SkullKind,
};
pub use builtin::BuiltinModelRegistry;
pub use error::ModelError;
pub use item::{bake_item_from_png_bytes, bake_item_generated_model, ItemMeshOptions};
pub use parser::{
    mesh_to_obj_string, normalize_canonical_blockstate, BakedObjFace, BlockModelJson, BlockState,
    BlockStateDefinition, BlockStateResolver, GuiLight, ItemDisplayContext, ItemTransforms,
    ModelOverride, ModelTransform, ResolvedBlockModel, RotationJson, RotationValue, VariantModel,
};
