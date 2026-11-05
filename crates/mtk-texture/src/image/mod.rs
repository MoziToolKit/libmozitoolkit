mod alpha;
mod buffer;
mod loader;
mod padding;
mod palette;

pub use alpha::{batch_analyze_transparent_faces_f32, sample_alpha_f32, SampleMode};
pub use buffer::RgbaBuffer;
pub use loader::DecodedSprite;
pub(crate) use padding::apply_edge_clamping_padding;
pub use palette::bake_paletted_permutation;
