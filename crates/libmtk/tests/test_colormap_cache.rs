//! Verifies that resource-pack colormaps are installed from an opened cache.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use libmtk::AssetCacheReader;
use mtk_package::{ChunkWriteOptions, MtkPackageWriter, PackageProfile};

fn solid_png(rgb: [u8; 3]) -> Vec<u8> {
    let image = RgbImage::from_pixel(1, 1, Rgb(rgb));
    let mut buffer = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut buffer, ImageFormat::Png)
        .expect("PNG encode");
    buffer.into_inner()
}

#[test]
fn cache_colormap_override_is_installed_on_open() {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, [0x42; 16]).unwrap();
    let red = solid_png([255, 0, 0]);
    writer
        .add_chunk(
            *b"BIOM",
            "biome/colormap/grass",
            &red,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();
    writer.finish().unwrap();

    let path = std::env::temp_dir().join(format!(
        "mtk_colormap_cache_{}_{}.mtkcache",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, cursor.into_inner()).unwrap();

    let reader = AssetCacheReader::open(&path).expect("open cache");
    // Verify open is side-effect-free (does not mutate global ACTIVE_COLORMAPS)
    let default_colormaps = mtk_material::get_default_colormaps();
    assert_ne!(
        default_colormaps.sample_grass_linear(0.5, 0.5),
        [1.0, 0.0, 0.0, 1.0]
    );

    // Verify load_colormaps loads them purely into an isolated instance
    let loaded = reader
        .load_colormaps()
        .expect("load colormaps")
        .expect("present colormaps");
    assert_eq!(loaded.sample_grass_linear(0.5, 0.5), [1.0, 0.0, 0.0, 1.0]);

    // Verify explicit install_colormaps installs into the global cache
    reader.install_colormaps().unwrap();
    let colormaps = mtk_material::get_default_colormaps();
    assert_eq!(
        colormaps.sample_grass_linear(0.5, 0.5),
        [1.0, 0.0, 0.0, 1.0]
    );

    mtk_material::clear_cached_colormaps();
    let _ = std::fs::remove_file(path);
}
