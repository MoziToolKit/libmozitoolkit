#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Identifies biome or model tinting behavior.
/// -1 represents no tint. >= 0 references tint index in Minecraft model element face.
pub type TintIndex = i16;

/// Material slot identifier, mapped to shader material or atlas slice.
pub type MaterialSlotId = u16;

/// Light level encoding block light (0..15) and sky light (0..15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LightLevel {
    pub block: u8,
    pub sky: u8,
}

impl LightLevel {
    pub const ZERO: Self = Self { block: 0, sky: 0 };
    pub const MAX: Self = Self { block: 15, sky: 15 };

    #[inline]
    pub const fn new(block: u8, sky: u8) -> Self {
        Self {
            block: if block > 15 { 15 } else { block },
            sky: if sky > 15 { 15 } else { sky },
        }
    }
}

/// Standard geometric attribute domain corresponding to modern DCC & USD standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AttributeDomain {
    /// Per-vertex / point data (size = vertex_count, e.g. positions, normals, weights).
    #[default]
    Point,
    /// Per-face-corner / loop data (size = loop_count / indices.len(), e.g. UVs, corner colors).
    Corner,
    /// Per-face / polygon data (size = face_count, e.g. material_slot, chunk_id, tint_index, texture_key).
    Face,
    /// Constant / mesh-level global data (size = 1).
    Mesh,
}

impl AttributeDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Corner => "corner",
            Self::Face => "face",
            Self::Mesh => "mesh",
        }
    }
}

/// Dynamic, contiguous typed attribute storage for zero-copy memory operations.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AttributeData {
    Float(Vec<f32>),
    Float2(Vec<[f32; 2]>),
    Float3(Vec<[f32; 3]>),
    Float4(Vec<[f32; 4]>),
    Int8(Vec<i8>),
    Int16(Vec<i16>),
    Int32(Vec<i32>),
    UInt8(Vec<u8>),
    UInt16(Vec<u16>),
    UInt32(Vec<u32>),
    String(Vec<String>),
    Bool(Vec<bool>),
}

impl AttributeData {
    /// Number of elements in this attribute array.
    pub fn len(&self) -> usize {
        match self {
            Self::Float(v) => v.len(),
            Self::Float2(v) => v.len(),
            Self::Float3(v) => v.len(),
            Self::Float4(v) => v.len(),
            Self::Int8(v) => v.len(),
            Self::Int16(v) => v.len(),
            Self::Int32(v) => v.len(),
            Self::UInt8(v) => v.len(),
            Self::UInt16(v) => v.len(),
            Self::UInt32(v) => v.len(),
            Self::String(v) => v.len(),
            Self::Bool(v) => v.len(),
        }
    }

    /// Whether this attribute array is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates an empty attribute of the same variant, pre-allocated for `capacity` elements.
    ///
    /// Used by mesh merging to size destination buffers once instead of growing them
    /// repeatedly while concatenating many section meshes.
    pub fn empty_with_capacity(&self, capacity: usize) -> Self {
        match self {
            Self::Float(_) => Self::Float(Vec::with_capacity(capacity)),
            Self::Float2(_) => Self::Float2(Vec::with_capacity(capacity)),
            Self::Float3(_) => Self::Float3(Vec::with_capacity(capacity)),
            Self::Float4(_) => Self::Float4(Vec::with_capacity(capacity)),
            Self::Int8(_) => Self::Int8(Vec::with_capacity(capacity)),
            Self::Int16(_) => Self::Int16(Vec::with_capacity(capacity)),
            Self::Int32(_) => Self::Int32(Vec::with_capacity(capacity)),
            Self::UInt8(_) => Self::UInt8(Vec::with_capacity(capacity)),
            Self::UInt16(_) => Self::UInt16(Vec::with_capacity(capacity)),
            Self::UInt32(_) => Self::UInt32(Vec::with_capacity(capacity)),
            Self::String(_) => Self::String(Vec::with_capacity(capacity)),
            Self::Bool(_) => Self::Bool(Vec::with_capacity(capacity)),
        }
    }

    /// Returns a string identifier of the data type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Float(_) => "float",
            Self::Float2(_) => "float2",
            Self::Float3(_) => "float3",
            Self::Float4(_) => "float4",
            Self::Int8(_) => "int8",
            Self::Int16(_) => "int16",
            Self::Int32(_) => "int32",
            Self::UInt8(_) => "uint8",
            Self::UInt16(_) => "uint16",
            Self::UInt32(_) => "uint32",
            Self::String(_) => "string",
            Self::Bool(_) => "bool",
        }
    }

    /// Read-only slice of raw underlying contiguous bytes (None for non-POD types like String).
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Float(v) => Some(bytemuck::cast_slice(v)),
            Self::Float2(v) => Some(bytemuck::cast_slice(v)),
            Self::Float3(v) => Some(bytemuck::cast_slice(v)),
            Self::Float4(v) => Some(bytemuck::cast_slice(v)),
            Self::Int8(v) => Some(bytemuck::cast_slice(v)),
            Self::Int16(v) => Some(bytemuck::cast_slice(v)),
            Self::Int32(v) => Some(bytemuck::cast_slice(v)),
            Self::UInt8(v) => Some(v.as_slice()),
            Self::UInt16(v) => Some(bytemuck::cast_slice(v)),
            Self::UInt32(v) => Some(bytemuck::cast_slice(v)),
            Self::Bool(v) => Some(bytemuck::cast_slice(v)),
            Self::String(_) => None,
        }
    }
}

/// Generic mesh attribute definition.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MeshAttribute {
    pub name: String,
    pub domain: AttributeDomain,
    pub data: AttributeData,
}

impl MeshAttribute {
    pub fn new(name: impl Into<String>, domain: AttributeDomain, data: AttributeData) -> Self {
        Self {
            name: name.into(),
            domain,
            data,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.data.as_bytes()
    }
}

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

/// Face-level metadata attached to baked geometry.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FaceAttributes {
    /// Texture identifier or atlas sprite key.
    pub texture_key: String,
    /// Material slot index.
    pub material_slot: MaterialSlotId,
    /// Tint index for colormap or biome blending (-1 for none).
    pub tint_index: TintIndex,
    /// Whether this face is an overlay layer (e.g. grass side overlay).
    pub is_overlay: bool,
    /// Atlas chunk/tile ID if mapped into a global atlas.
    pub atlas_chunk_id: Option<u32>,
    /// Packed UV affine transform [scale_u, scale_v, offset_u, offset_v].
    pub uv_transform: [f32; 4],
    /// UV rotation angle in radians.
    pub uv_rotation: f32,
    /// Animation timing [total_frames, frametime, interpolate].
    pub anim_timing: [f32; 3],
    /// Animation frame size [frame_width, frame_height, 0].
    pub anim_frame_size: [f32; 3],
    /// Resolved linear RGBA tint color [r, g, b, a].
    pub biome_tint_color: [f32; 4],
    /// Packed biome tint data [base_weight, overlay_weight, tint_weight, tint_type].
    pub biome_tint_data: [f32; 4],
    /// Minecraft absolute block coordinate [x, y, z].
    pub block_pos: [i32; 3],
    /// Light level.
    pub light_level: LightLevel,
}

impl Default for FaceAttributes {
    fn default() -> Self {
        Self {
            texture_key: String::new(),
            material_slot: 0,
            tint_index: -1,
            is_overlay: false,
            atlas_chunk_id: None,
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            uv_rotation: 0.0,
            anim_timing: [1.0, 1.0, 0.0],
            anim_frame_size: [16.0, 16.0, 0.0],
            biome_tint_color: [1.0, 1.0, 1.0, 1.0],
            biome_tint_data: [1.0, 1.0, 0.0, 0.0],
            block_pos: [0, 0, 0],
            light_level: LightLevel::ZERO,
        }
    }
}

impl FaceAttributes {
    /// Creates a new `FaceAttributes` with given texture key and material slot.
    pub fn new(texture_key: impl Into<String>, material_slot: MaterialSlotId) -> Self {
        Self {
            texture_key: texture_key.into(),
            material_slot,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_data_as_bytes() {
        let f = AttributeData::Float(vec![1.0, 2.0]);
        let bytes = f.as_bytes().unwrap();
        assert_eq!(bytes.len(), 8);

        let f3 = AttributeData::Float3(vec![[1.0, 2.0, 3.0]]);
        assert_eq!(f3.as_bytes().unwrap().len(), 12);

        let b = AttributeData::Bool(vec![true, false, true]);
        let bytes = b.as_bytes().unwrap();
        assert_eq!(bytes, &[1, 0, 1]);

        let s = AttributeData::String(vec!["hello".to_string()]);
        assert!(s.as_bytes().is_none());
    }

    #[test]
    fn test_canonical_attribute_specs_are_unique_and_typed() {
        let mut names = std::collections::HashSet::new();
        for spec in MESH_ATTRIBUTE_SPECS {
            assert!(
                spec.name.starts_with("mtk_"),
                "unexpected name: {}",
                spec.name
            );
            assert!(
                names.insert(spec.name),
                "duplicate canonical attribute: {}",
                spec.name
            );
            assert!(!spec.data_type.is_empty());
            assert_ne!(spec.role, AttributeRole::Retired);
        }
    }

    #[test]
    fn test_retired_attributes_are_disjoint_from_active_schema() {
        let active: std::collections::HashSet<&str> =
            MESH_ATTRIBUTE_SPECS.iter().map(|spec| spec.name).collect();
        let mut retired = std::collections::HashSet::new();
        for name in RETIRED_MESH_ATTRIBUTES {
            assert!(retired.insert(*name), "duplicate retired attribute: {name}");
            assert!(
                !active.contains(name),
                "attribute {name} is both active and retired"
            );
        }
    }

    #[test]
    fn test_attribute_role_strings_are_stable() {
        assert_eq!(AttributeRole::Material.as_str(), "material");
        assert_eq!(AttributeRole::Metadata.as_str(), "metadata");
        assert_eq!(AttributeRole::Retired.as_str(), "retired");
    }
}
