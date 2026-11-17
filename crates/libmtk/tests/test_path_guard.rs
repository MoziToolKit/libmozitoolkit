use std::fs;
use std::path::PathBuf;

use libmtk::AssetCacheReader;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, PackageProfile};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "mtk_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_path_guard_rejects_escape_in_extract_all_atlas_textures() {
    let temp = TempDirGuard::new("path_guard_extract_all");
    let cache_file = temp.path.join("hostile.mtkcache");

    // 1. Create a package with an ATLS chunk containing ".." in the identifier
    let mut writer =
        MtkPackageFileWriter::create(&cache_file, PackageProfile::AssetCache, [0x55; 16]).unwrap();

    writer
        .add_chunk(
            *b"ATLS",
            "atlas/textures/../../escape.png",
            b"PNG_DATA",
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    writer.finish().unwrap();

    // 2. Open package and attempt extraction
    let reader = AssetCacheReader::open(&cache_file).unwrap();
    let out_dir = temp.path.join("output");
    let escape_target = temp.path.join("escape.png");

    let res = reader.extract_all_atlas_textures(&out_dir);
    assert!(
        res.is_err(),
        "extract_all_atlas_textures must return Err when a chunk contains '..'"
    );

    // 3. Verify no file was written outside output directory
    assert!(
        !escape_target.exists(),
        "Malicious file must not be written outside output directory"
    );
}
