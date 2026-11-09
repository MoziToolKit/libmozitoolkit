//! Canonical built-in mesh attribute schema and retired name registry.

use super::types::AttributeDomain;

/// Canonical built-in mesh attribute names (libmtk Spec v2).
///
/// This module is the single source of truth for the mesh attribute contract.
/// Hosts (Blender / WASM / CLI) must consume [`MESH_ATTRIBUTE_SPECS`] and
/// [`RETIRED_MESH_ATTRIBUTES`] instead of maintaining their own name lists.
pub mod constants {
    /// Canonical source texture key (Domain: Face, Type: String).
    /// DEPRECATED: Retired in favor of [`ATTR_SOURCE_TEXTURE_IDX`] and [`ATTR_SOURCE_TEXTURE_PALETTE`].
    pub const ATTR_SOURCE_TEXTURE_KEY: &str = "mtk_source_texture_key";
    /// Palette index for source textures (Domain: Face, Type: Int32).
    /// Produced by the native mesher or host from the native source-texture palette; kept in the
    /// canonical schema so every consumer agrees on its type and role.
    pub const ATTR_SOURCE_TEXTURE_IDX: &str = "mtk_source_texture_idx";
    /// Source texture string palette (Domain: Mesh, Type: String).
    pub const ATTR_SOURCE_TEXTURE_PALETTE: &str = "mtk_source_textures";

    /// Target Atlas chunk/sheet slot index (Domain: Face, Type: Int32).
    pub const ATTR_ATLAS_CHUNK_ID: &str = "mtk_atlas_chunk_id";

    /// Packed UV affine transform (Domain: Face, Type: Float4/RGBA [scale_u, scale_v, offset_u, offset_v]).
    pub const ATTR_UV_TRANSFORM: &str = "mtk_uv_transform";
    /// Dynamic UV rotation angle in radians (Domain: Face, Type: Float32).
    pub const ATTR_UV_ROTATION: &str = "mtk_uv_rotation";

    /// Animation timing parameters (Domain: Face, Type: Float3 [total_frames, frametime, interpolate]).
    pub const ATTR_ANIM_TIMING: &str = "mtk_anim_timing";
    /// Animation frame size (Domain: Face, Type: Float3 [frame_width, frame_height, 0]).
    pub const ATTR_ANIM_FRAME_SIZE: &str = "mtk_anim_frame_size";

    /// Corner ambient occlusion / smooth lighting factor (Domain: Corner, Type: Float32 [0.0..1.0]).
    pub const ATTR_AO: &str = "mtk_ao";

    /// Resolved linear RGBA biome tint color (Domain: Face/Corner, Type: Float4).
    pub const ATTR_BIOME_TINT_COLOR: &str = "mtk_biome_tint_color";
    /// Packed biome tint data and weights (Domain: Face, Type: Float4 [base_weight, overlay_weight, tint_weight, tint_type]).
    pub const ATTR_BIOME_TINT_DATA: &str = "mtk_biome_tint_data";

    /// Minecraft absolute world coordinates (Domain: Face/Point, Type: Int32 x 3).
    pub const ATTR_BLOCK_X: &str = "mtk_block_x";
    pub const ATTR_BLOCK_Y: &str = "mtk_block_y";
    pub const ATTR_BLOCK_Z: &str = "mtk_block_z";

    /// Voxel point cloud block state index into [`ATTR_PALETTE`] (Domain: Point, Type: Int32).
    pub const ATTR_STATE_IDX: &str = "mtk_state_idx";
    /// Voxel point cloud block state string palette (Domain: Mesh, Type: String).
    pub const ATTR_PALETTE: &str = "mtk_palette";

    /// Voxel point cloud biome index into [`ATTR_BIOME_PALETTE`] (Domain: Point, Type: Int32).
    pub const ATTR_BIOME_IDX: &str = "mtk_biome_idx";
    /// Voxel point cloud biome string palette (Domain: Mesh, Type: String).
    pub const ATTR_BIOME_PALETTE: &str = "mtk_biome_palette";

    /// Voxel point cloud block entity index into [`ATTR_ENTITY_PALETTE`] (Domain: Point, Type: Int32, -1 for none).
    pub const ATTR_ENTITY_IDX: &str = "mtk_entity_idx";
    /// Voxel point cloud block entity NBT/JSON string palette (Domain: Mesh, Type: String).
    pub const ATTR_ENTITY_PALETTE: &str = "mtk_entity_palette";

    /// Voxel point cloud light level (Domain: Point, Type: UInt8).
    pub const ATTR_LIGHT: &str = "mtk_light";

    /// Voxel point cloud coordinate bounding box [min_x, min_y, min_z, max_x, max_y, max_z] (Domain: Mesh, Type: Int32 x 6).
    pub const ATTR_BOUNDS: &str = "mtk_bounds";
}

/// High-level role of a canonical mesh attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeRole {
    /// Read by DCC shader node graphs (must be injected as a mesh attribute).
    Material,
    /// Read by host-side metadata logic (provenance, atlas routing, spatial sampling).
    Metadata,
    /// No longer produced; listed only so hosts can purge it from legacy files.
    Retired,
}

impl AttributeRole {
    /// Stable string identifier used by bindings and generated host schemas.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Material => "material",
            Self::Metadata => "metadata",
            Self::Retired => "retired",
        }
    }
}

/// Static declaration of one mesh attribute in the canonical schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshAttributeSpec {
    /// Attribute name as written into DCC / mesh data.
    pub name: &'static str,
    /// Attribute domain.
    pub domain: AttributeDomain,
    /// Portable data type name (matches [`AttributeData::type_name`]).
    pub data_type: &'static str,
    /// Whether the attribute feeds material nodes, host metadata logic, or neither.
    pub role: AttributeRole,
}

impl MeshAttributeSpec {
    const fn new(
        name: &'static str,
        domain: AttributeDomain,
        data_type: &'static str,
        role: AttributeRole,
    ) -> Self {
        Self {
            name,
            domain,
            data_type,
            role,
        }
    }
}

/// Canonical active mesh attribute schema.
///
/// Only attributes listed here may be produced by the native core or injected by
/// a host. The order is stable so generated host bindings stay deterministic.
pub const MESH_ATTRIBUTE_SPECS: &[MeshAttributeSpec] = &[
    MeshAttributeSpec::new(
        constants::ATTR_SOURCE_TEXTURE_IDX,
        AttributeDomain::Face,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_SOURCE_TEXTURE_PALETTE,
        AttributeDomain::Mesh,
        "string",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_ATLAS_CHUNK_ID,
        AttributeDomain::Face,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_UV_TRANSFORM,
        AttributeDomain::Face,
        "float4",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_UV_ROTATION,
        AttributeDomain::Face,
        "float",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_ANIM_TIMING,
        AttributeDomain::Face,
        "float3",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_ANIM_FRAME_SIZE,
        AttributeDomain::Face,
        "float3",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_AO,
        AttributeDomain::Corner,
        "float",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BIOME_TINT_COLOR,
        AttributeDomain::Corner,
        "float4",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BIOME_TINT_DATA,
        AttributeDomain::Face,
        "float4",
        AttributeRole::Material,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BLOCK_X,
        AttributeDomain::Face,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BLOCK_Y,
        AttributeDomain::Face,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BLOCK_Z,
        AttributeDomain::Face,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_STATE_IDX,
        AttributeDomain::Point,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_PALETTE,
        AttributeDomain::Mesh,
        "string",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BIOME_IDX,
        AttributeDomain::Point,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BIOME_PALETTE,
        AttributeDomain::Mesh,
        "string",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_ENTITY_IDX,
        AttributeDomain::Point,
        "int32",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_ENTITY_PALETTE,
        AttributeDomain::Mesh,
        "string",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_LIGHT,
        AttributeDomain::Point,
        "uint8",
        AttributeRole::Metadata,
    ),
    MeshAttributeSpec::new(
        constants::ATTR_BOUNDS,
        AttributeDomain::Mesh,
        "int32",
        AttributeRole::Metadata,
    ),
];

/// Mesh attributes that must never be written again.
///
/// Hosts use this list to purge stale attributes from legacy `.blend` files.
/// Nothing in this list may appear in [`MESH_ATTRIBUTE_SPECS`].
pub const RETIRED_MESH_ATTRIBUTES: &[&str] = &[
    // Legacy per-face string attribute (retired in favor of compact Palette + ATTR_SOURCE_TEXTURE_IDX).
    constants::ATTR_SOURCE_TEXTURE_KEY,
    // Obsolete atlas routing / material metadata.
    "mtk_material_slot",
    "mtk_material_id",
    "mtk_atlas_texture_id",
    "mtk_uv_mode",
    "mtk_source_origin",
    "mtk_is_opaque",
    "mtk_alpha_mode",
    // Emission moved into the pre-decoded ORM alpha channel; per-face value is dead.
    "mtk_emission",
    // Face direction is redundant with the polygon normal.
    "mtk_face_dir",
    // Biome colormap sampling now resolves Linear RGBA directly in the native core.
    "mtk_colormap_uv",
    "mtk_tint_weight",
    "mtk_base_tint_weight",
    "mtk_overlay_tint_weight",
    "mtk_tint_color",
    "mtk_tint_type",
    "mtk_hardcoded_color",
    "mtk_use_hardcoded",
    "mtk_biome_temperature",
    "mtk_biome_humidity",
    "mtk_biome_coords",
    // Scalar animation splits replaced by mtk_anim_timing / mtk_anim_frame_size.
    "mtk_anim_total_frames",
    "mtk_anim_frametime",
    "mtk_anim_interpolate",
    "mtk_anim_frame_width",
    "mtk_anim_frame_height",
    // Legacy UV tiling replaced by mtk_uv_transform.
    "mtk_uv_tiling_scale",
    "mtk_uv_tiling_location",
    "mtk_uv_tiling_transform",
    // Retired physical-property table and legacy point-cloud strings.
    "mtk_material_props",
    "mtk_block_state",
    "mtk_biome",
    // Legacy unnamespaced names.
    "atlas_chunk_id",
    "atlas_texture_id",
    "material_id",
];
