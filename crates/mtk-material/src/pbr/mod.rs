//! # PBR Decoding and Packing
//!
//! Source-format decoders (LabPBR 1.3, future Bedrock MER / glTF metallic-roughness)
//! produce the canonical `MtkSurface` channels defined in [`types`]. Target packers
//! (Blender, OpenPBR, UsdPreviewSurface, Bedrock) consume those channels.
//!
//! The pure decoder lives here rather than in `mtk-texture` because `mtk-material`
//! already depends on `mtk-texture`; the reverse dependency would form a cycle.

mod bake;
mod labpbr;
pub(crate) mod preset;
mod types;

pub use bake::decode_baked_atlas;
