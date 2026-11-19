mod color;

pub(crate) use color::BIOME_KERNEL_R2;
pub use color::{get_point_column_biome, get_smoothed_column_biome, SmoothedBiomeColumn};
pub(crate) use color::{BiomeColumnSums, BiomeTintScratch};
