//! # Canonical PBR Channel Types
//!
//! `MtkSurface` 是宿主无关的 canonical PBR 通道模型；`MtkPackedV1` 是它在
//! `.mtkcache` 中的紧凑贴图布局。详见 `MoziToolKit/docs/materials/pbr_channel_spec.md`。
//!
//! 布局/元数据类型定义在 `mtk-texture`，因为 `AtlasChunkMeta` 需要序列化
//! 它们；这里只保留解码结果与 re-export，避免 `mtk-texture -> mtk-material` 成环。

use mtk_texture::RgbaBuffer;

/// Decoded canonical channel buffers for one atlas chunk (or one standalone sprite).
///
/// Missing source companions produce missing output layers:
/// - no normal and no specular -> all `None`
/// - normal only -> `normal_gl` + `orm` (AO/height/default roughness)
/// - specular only -> `orm` + `aux`
/// - both -> all three
#[derive(Debug, Clone)]
pub(crate) struct DecodedPbr {
    /// RGB = OpenGL tangent normal, A = height.
    pub normal_gl: Option<RgbaBuffer>,
    /// R = AO, G = roughness, B = metallic, A = emission mask.
    pub orm: Option<RgbaBuffer>,
    /// R = encoded specular IOR, G = porosity, B = SSS weight, A = transmission mask (v1 = 0).
    pub aux: Option<RgbaBuffer>,
}
