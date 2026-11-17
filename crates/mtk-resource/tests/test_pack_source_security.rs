use mtk_resource::{ResourceError, ResourcePack, ZipPack};
use std::io::Write;

fn create_zip_with_declared_size(name: &str, declared_size: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(name, options).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
    }
    let size_bytes = declared_size.to_le_bytes();
    let mut i = 0;
    while i + 4 <= buf.len() {
        if &buf[i..i + 4] == b"PK\x03\x04" {
            buf[i + 22..i + 26].copy_from_slice(&size_bytes);
            i += 26;
        } else if &buf[i..i + 4] == b"PK\x01\x02" {
            buf[i + 24..i + 28].copy_from_slice(&size_bytes);
            i += 28;
        } else {
            i += 1;
        }
    }
    buf
}

#[test]
fn test_zip_entry_enormous_size_rejected_without_huge_allocation() {
    let zip_bytes = create_zip_with_declared_size("huge.bin", 512 * 1024 * 1024);
    let result = ZipPack::from_bytes("test_huge", &zip_bytes);
    assert!(
        result.is_err(),
        "A zip with an entry declaring an enormous size must be rejected"
    );
    match result.unwrap_err() {
        ResourceError::ZipEntryTooLarge { name, size, max } => {
            assert_eq!(name, "huge.bin");
            assert_eq!(size, 512 * 1024 * 1024);
            assert_eq!(max, ZipPack::MAX_ENTRY_SIZE);
        }
        other => panic!("Expected ZipEntryTooLarge error, got {:?}", other),
    }
}

#[test]
fn test_zip_path_safety_rejects_parent_dir_and_absolute_paths() {
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // Well-formed entry
        zip.start_file("assets/minecraft/textures/block/stone.png", options)
            .unwrap();
        zip.write_all(b"stone").unwrap();

        // Well-formed entry containing '..' in filename component (should be accepted)
        zip.start_file("assets/minecraft/textures/block/..stone.png", options)
            .unwrap();
        zip.write_all(b"dotdot_stone").unwrap();

        // Malicious entry: leading '..'
        zip.start_file("../evil.png", options).unwrap();
        zip.write_all(b"evil1").unwrap();

        // Malicious entry: '..' in path traversal
        zip.start_file("assets/../evil2.png", options).unwrap();
        zip.write_all(b"evil2").unwrap();

        // Malicious entry: absolute path with leading slash
        zip.start_file("/etc/passwd", options).unwrap();
        zip.write_all(b"root:x:0:0").unwrap();

        // Malicious entry: Windows drive prefix
        zip.start_file("C:/windows/win.ini", options).unwrap();
        zip.write_all(b"[fonts]").unwrap();

        // Malicious entry: Windows UNC / backslash absolute
        zip.start_file("\\windows\\system32\\cmd.exe", options)
            .unwrap();
        zip.write_all(b"cmd").unwrap();

        zip.finish().unwrap();
    }

    let pack = ZipPack::from_bytes("test_paths", &buf).expect("pack should load well-formed files");

    // Well-formed entries must be present
    assert!(
        pack.has_file("assets/minecraft/textures/block/stone.png"),
        "stone.png must be present"
    );
    assert_eq!(
        pack.open("assets/minecraft/textures/block/stone.png"),
        Some(b"stone".to_vec())
    );

    assert!(
        pack.has_file("assets/minecraft/textures/block/..stone.png"),
        "..stone.png is a normal component and must be preserved"
    );
    assert_eq!(
        pack.open("assets/minecraft/textures/block/..stone.png"),
        Some(b"dotdot_stone".to_vec())
    );

    // Malicious entries must be rejected (not present in pack)
    assert!(
        !pack.has_file("../evil.png"),
        "../evil.png must be rejected"
    );
    assert_eq!(pack.open("../evil.png"), None);

    assert!(
        !pack.has_file("assets/../evil2.png"),
        "assets/../evil2.png must be rejected"
    );
    assert_eq!(pack.open("assets/../evil2.png"), None);

    assert!(
        !pack.has_file("/etc/passwd"),
        "/etc/passwd must be rejected"
    );
    assert_eq!(pack.open("/etc/passwd"), None);

    assert!(
        !pack.has_file("C:/windows/win.ini"),
        "C:/windows/win.ini must be rejected"
    );
    assert_eq!(pack.open("C:/windows/win.ini"), None);

    assert!(
        !pack.has_file("\\windows\\system32\\cmd.exe"),
        "\\windows\\system32\\cmd.exe must be rejected"
    );
    assert_eq!(pack.open("\\windows\\system32\\cmd.exe"), None);

    // Enumerated files must only include well-formed paths
    let files = pack.list_files("");
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"assets/minecraft/textures/block/stone.png".to_string()));
    assert!(files.contains(&"assets/minecraft/textures/block/..stone.png".to_string()));
}
