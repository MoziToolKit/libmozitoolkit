//! Core geometric and typed mesh attribute definitions.

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
