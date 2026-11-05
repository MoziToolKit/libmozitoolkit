use mtk_package::{
    ChunkWriteOptions, MtkPackageFileWriter, MtkPackageReader, MtkPackageWriter, PackageProfile,
};
use std::io::Cursor;

#[test]
fn test_package_roundtrip_memory() {
    let mut cursor = Cursor::new(Vec::new());
    let fingerprint = [0xAB; 16];
    let mut writer =
        MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, fingerprint).unwrap();

    // 1. Zstd compressed chunk
    let sample_model_data = b"MODEL_DATA_BLOCK_1234567890_REPEAT_FOR_COMPRESSION_".repeat(20);
    writer
        .add_chunk(
            *b"MODL",
            "models/database",
            &sample_model_data,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();

    // 2. Raw uncompressed chunk (e.g. PNG texture)
    let sample_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
    writer
        .add_chunk(
            *b"ATLS",
            "atlas/chunk_001.png",
            &sample_png,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    // 3. JSON chunk
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct TestManifest {
        packs: Vec<String>,
        count: u32,
    }
    let manifest = TestManifest {
        packs: vec!["vanilla".to_string(), "faithful".to_string()],
        count: 42,
    };
    writer
        .add_json_chunk(
            *b"META",
            "manifest",
            &manifest,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();

    let header = writer.finish().unwrap();
    assert_eq!(header.chunk_count, 3);
    assert_eq!(header.fingerprint, fingerprint);

    let bytes = cursor.into_inner();
    assert!(bytes.len() > 64);

    // Read back
    let reader = MtkPackageReader::from_bytes(bytes).unwrap();
    assert_eq!(reader.header().profile, PackageProfile::AssetCache);
    assert_eq!(reader.fingerprint(), &fingerprint);
    assert_eq!(reader.header().chunk_count, 3);
    assert_eq!(reader.iter_chunk_identifiers(*b"MODL").count(), 1);
    assert_eq!(reader.iter_chunk_identifiers(*b"ATLS").count(), 1);
    assert_eq!(reader.iter_chunk_identifiers(*b"META").count(), 1);

    // Verify data integrity
    let read_models = reader.read_chunk_decompressed("models/database").unwrap();
    assert_eq!(read_models, sample_model_data);

    let read_png = reader
        .read_chunk_decompressed("atlas/chunk_001.png")
        .unwrap();
    assert_eq!(read_png, sample_png);

    let read_manifest: TestManifest = reader.read_chunk_json("manifest").unwrap();
    assert_eq!(read_manifest, manifest);
}

#[test]
fn test_package_file_and_header_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.mtkcache");

    let fingerprint = [0x55; 16];
    let mut writer =
        MtkPackageFileWriter::create(&file_path, PackageProfile::AssetCache, fingerprint).unwrap();

    writer
        .add_str_chunk(
            *b"BIOM",
            "biome/mapping",
            r#"{"biomes":["plains","forest"]}"#,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();

    writer.finish().unwrap();

    // Verify header-only reading (microsecond check)
    let header_only = MtkPackageReader::read_header_only(&file_path).unwrap();
    assert_eq!(header_only.fingerprint, fingerprint);
    assert_eq!(header_only.chunk_count, 1);

    // Verify mmap reader
    let reader = MtkPackageReader::open_file(&file_path).unwrap();
    assert!(reader.has_chunk("biome/mapping"));
    let str_val = reader.read_chunk_str("biome/mapping").unwrap();
    assert_eq!(str_val, r#"{"biomes":["plains","forest"]}"#);

    // Verify buffered reader (safe path)
    let buffered_reader = MtkPackageReader::open_file_buffered(&file_path).unwrap();
    assert!(buffered_reader.has_chunk("biome/mapping"));
    let str_val_buf = buffered_reader.read_chunk_str("biome/mapping").unwrap();
    assert_eq!(str_val_buf, r#"{"biomes":["plains","forest"]}"#);
}

fn temp_package_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
        .collect()
}

/// A rebuild must never truncate the package that is currently being read.
/// The replacement is staged in a sibling file and published with a rename,
/// which is atomic on POSIX and avoids `ERROR_USER_MAPPED_FILE` on Windows.
#[test]
fn test_file_writer_publishes_atomically() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("atomic.mtkcache");

    let mut writer =
        MtkPackageFileWriter::create(&file_path, PackageProfile::AssetCache, [0x11; 16]).unwrap();
    writer
        .add_str_chunk(*b"BIOM", "version", "v1", &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    // A reader opened before the rebuild keeps a valid snapshot of v1.
    let before = MtkPackageReader::open_file(&file_path).unwrap();
    assert_eq!(before.read_chunk_str("version").unwrap(), "v1");

    let mut writer =
        MtkPackageFileWriter::create(&file_path, PackageProfile::AssetCache, [0x22; 16]).unwrap();
    writer
        .add_str_chunk(*b"BIOM", "version", "v2", &ChunkWriteOptions::raw())
        .unwrap();

    // While v2 is being staged the published package is still v1.
    assert_eq!(
        MtkPackageReader::open_file(&file_path)
            .unwrap()
            .read_chunk_str("version")
            .unwrap(),
        "v1"
    );
    assert_eq!(temp_package_files(temp_dir.path()).len(), 1);

    // Windows cannot replace a destination that still has an open mapped view.
    drop(before);

    writer.finish().unwrap();
    assert!(temp_package_files(temp_dir.path()).is_empty());
    assert_eq!(
        MtkPackageReader::open_file(&file_path)
            .unwrap()
            .read_chunk_str("version")
            .unwrap(),
        "v2"
    );
}

/// Dropping a writer without finishing (e.g. a failed rebuild) removes the
/// temporary file and leaves the previously published package intact.
#[test]
fn test_dropped_file_writer_keeps_previous_package() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("keep.mtkcache");

    let mut writer =
        MtkPackageFileWriter::create(&file_path, PackageProfile::AssetCache, [0x33; 16]).unwrap();
    writer
        .add_str_chunk(*b"BIOM", "version", "v1", &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    {
        let mut abandoned =
            MtkPackageFileWriter::create(&file_path, PackageProfile::AssetCache, [0x44; 16])
                .unwrap();
        abandoned
            .add_str_chunk(*b"BIOM", "version", "v2", &ChunkWriteOptions::raw())
            .unwrap();
        // Dropped without `finish()`.
    }

    assert!(temp_package_files(temp_dir.path()).is_empty());
    assert_eq!(
        MtkPackageReader::open_file(&file_path)
            .unwrap()
            .read_chunk_str("version")
            .unwrap(),
        "v1"
    );
}
