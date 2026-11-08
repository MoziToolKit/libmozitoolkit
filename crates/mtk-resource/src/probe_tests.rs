    use super::*;
    use std::io::Write;

    #[test]
    fn test_empty_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("empty.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let _zip = zip::ZipWriter::new(file);
        }
        let probe = probe_pack(&zip_path);
        assert_eq!(probe.kind, KIND_UNKNOWN);
        assert_eq!(probe.error.as_deref(), Some("archive is empty"));
    }

    #[test]
    fn test_corrupt_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("corrupt.jar");
        fs::write(&zip_path, b"not a valid zip").unwrap();
        let probe = probe_pack(&zip_path);
        assert_eq!(probe.kind, KIND_UNKNOWN);
        assert!(probe.error.unwrap().to_lowercase().contains("zip"));
    }

    #[test]
    fn test_resource_pack_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("pack.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("pack.mcmeta", options).unwrap();
            zip.write_all(br#"{"pack": {"pack_format": 88, "description": "Test Pack"}}"#)
                .unwrap();

            zip.start_file("pack.png", options).unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\n").unwrap();

            zip.start_file("assets/minecraft/textures/block/stone.png", options)
                .unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\n").unwrap();
            zip.finish().unwrap();
        }

        let probe = probe_pack(&zip_path);
        assert_eq!(probe.kind, KIND_RESOURCE_PACK);
        assert_eq!(probe.suggested_pack_type.as_deref(), Some("RESOURCE_PACK"));
        assert_eq!(probe.details.get("pack_format").unwrap().as_i64(), Some(88));
        assert!(probe.asset_summary.has_block_assets);
        assert!(probe.asset_summary.has_renderable_assets);
        assert_eq!(probe.asset_summary.block_texture_count, 1);
        assert_eq!(probe.asset_summary.namespaces, vec!["minecraft"]);
    }

    #[test]
    fn test_functional_mod_has_no_renderable_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("sodium.jar");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(br#"{"schemaVersion": 1, "id": "sodium", "name": "Sodium"}"#)
                .unwrap();

            zip.start_file("assets/sodium/icon.png", options).unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\n").unwrap();

            zip.start_file("assets/sodium/lang/en_us.json", options)
                .unwrap();
            zip.write_all(b"{}").unwrap();

            zip.finish().unwrap();
        }

        let probe = probe_pack(&zip_path);
        assert_eq!(probe.kind, KIND_MOD);
        assert_eq!(probe.suggested_pack_type.as_deref(), Some("MOD_JAR"));
        assert!(!probe.asset_summary.has_block_assets);
        assert!(!probe.asset_summary.has_renderable_assets);
        assert_eq!(probe.asset_summary.blockstate_count, 0);
        assert_eq!(probe.asset_summary.block_model_count, 0);
    }

    #[test]
    fn test_block_mod_has_renderable_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("waystones.jar");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(br#"{"schemaVersion": 1, "id": "waystones", "name": "Waystones"}"#)
                .unwrap();

            zip.start_file("assets/waystones/blockstates/waystone.json", options)
                .unwrap();
            zip.write_all(b"{}").unwrap();

            zip.start_file("assets/waystones/models/block/waystone.json", options)
                .unwrap();
            zip.write_all(b"{}").unwrap();

            zip.start_file("assets/waystones/textures/block/waystone.png", options)
                .unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\n").unwrap();

            zip.finish().unwrap();
        }

        let probe = probe_pack(&zip_path);
        assert_eq!(probe.kind, KIND_MOD);
        assert!(probe.asset_summary.has_block_assets);
        assert!(probe.asset_summary.has_renderable_assets);
        assert_eq!(probe.asset_summary.blockstate_count, 1);
        assert_eq!(probe.asset_summary.block_model_count, 1);
        assert_eq!(probe.asset_summary.block_texture_count, 1);
        assert_eq!(probe.asset_summary.namespaces, vec!["waystones"]);
    }
