use std::io::Cursor;
use tempfile::tempdir;

use mtk_package::{
    ChunkWriteOptions, MtkPackageReader, MtkPackageWriter, PackageError, PackageProfile,
};

fn create_sample_package() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, [0x42; 16]).unwrap();

    writer
        .add_chunk(
            *b"TEST",
            "test/chunk_a",
            b"Sample chunk A content",
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    writer.finish().unwrap();
    cursor.into_inner()
}

fn create_two_chunk_package() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, [0x42; 16]).unwrap();

    writer
        .add_chunk(
            *b"TEST",
            "test/chunk_a",
            b"Sample chunk A content",
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    writer
        .add_chunk(
            *b"TEST",
            "test/chunk_b",
            b"Sample chunk B content",
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    writer.finish().unwrap();
    cursor.into_inner()
}

fn modify_toc<F: FnOnce(&mut Vec<u8>)>(package_bytes: &mut Vec<u8>, f: F) {
    let toc_offset = u64::from_le_bytes(package_bytes[40..48].try_into().unwrap()) as usize;
    let toc_length = u64::from_le_bytes(package_bytes[48..56].try_into().unwrap()) as usize;
    let is_zstd = (package_bytes[10] & (1 << 1)) != 0;

    let mut raw_toc = if is_zstd {
        zstd::decode_all(&package_bytes[toc_offset..toc_offset + toc_length]).unwrap()
    } else {
        package_bytes[toc_offset..toc_offset + toc_length].to_vec()
    };

    f(&mut raw_toc);

    let new_toc_bytes = if is_zstd {
        zstd::encode_all(Cursor::new(raw_toc), 3).unwrap()
    } else {
        raw_toc
    };

    package_bytes.truncate(toc_offset);
    package_bytes.extend_from_slice(&new_toc_bytes);
    package_bytes[48..56].copy_from_slice(&(new_toc_bytes.len() as u64).to_le_bytes());
    let crc = crc32fast::hash(&package_bytes[0..60]);
    package_bytes[60..64].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn test_chunk_huge_declared_decompressed_size_rejected() {
    let mut bytes = create_sample_package();

    // In raw_toc:
    // Entry 0 decompressed_size is at bytes 24..32
    modify_toc(&mut bytes, |raw_toc| {
        let huge_size: u64 = 50 * 1024 * 1024 * 1024; // 50 GiB
        raw_toc[24..32].copy_from_slice(&huge_size.to_le_bytes());
    });

    let reader = MtkPackageReader::from_bytes(bytes).expect("TOC parsing should succeed");
    let err = reader
        .read_chunk_decompressed("test/chunk_a")
        .expect_err("Chunk with 50 GiB declared size must be rejected without allocation");

    match err {
        PackageError::DecompressionFailed { source, .. } => {
            assert_eq!(source.kind(), std::io::ErrorKind::InvalidData);
        }
        other => panic!("Expected DecompressionFailed error, got {:?}", other),
    }
}

#[test]
fn test_decompression_bomb_exceeding_64mib_cap_rejected() {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, [0x42; 16]).unwrap();

    // 70 MiB of compressible data: compresses to a few hundred bytes
    let bomb_payload = vec![0xABu8; 70 * 1024 * 1024];
    writer
        .add_chunk(
            *b"BOMB",
            "test/bomb",
            &bomb_payload,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();

    writer.finish().unwrap();
    let mut bytes = cursor.into_inner();

    // Case 1: declared size is 70 MiB (exceeds 64 MiB cap)
    let reader = MtkPackageReader::from_bytes(bytes.clone()).unwrap();
    let res = reader.read_chunk_decompressed("test/bomb");
    assert!(
        res.is_err(),
        "Chunk exceeding 64 MiB declared size must be rejected"
    );

    // Case 2: forged TOC claiming decompressed_size is only 100 bytes, but payload expands to 70 MiB
    modify_toc(&mut bytes, |raw_toc| {
        let fake_size: u64 = 100;
        raw_toc[24..32].copy_from_slice(&fake_size.to_le_bytes());
    });
    let reader = MtkPackageReader::from_bytes(bytes).unwrap();
    let res = reader.read_chunk_decompressed("test/bomb");
    assert!(
        res.is_err(),
        "Decompression bomb with forged small declared size must be rejected"
    );
}

#[test]
fn test_decompression_size_mismatch_rejected() {
    let mut bytes = create_sample_package();

    // Corrupt declared decompressed_size to be 999 instead of actual content length
    modify_toc(&mut bytes, |raw_toc| {
        let mismatched_size: u64 = 999;
        raw_toc[24..32].copy_from_slice(&mismatched_size.to_le_bytes());
    });

    let reader = MtkPackageReader::from_bytes(bytes).unwrap();
    let res = reader.read_chunk_decompressed("test/chunk_a");
    assert!(res.is_err(), "Size mismatch must return Err");
}

#[test]
fn test_toc_entry_offset_below_header_size_rejected() {
    let mut bytes = create_sample_package();

    // Set chunk offset below header size (< 64)
    modify_toc(&mut bytes, |raw_toc| {
        let invalid_offset: u64 = 0;
        raw_toc[8..16].copy_from_slice(&invalid_offset.to_le_bytes());
    });

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::OutOfBounds { .. }) => {}
        Err(other) => panic!("Expected OutOfBounds for offset < 64, got {:?}", other),
        Ok(_) => panic!("Chunk with offset < 64 must be rejected"),
    }
}

#[test]
fn test_toc_entry_unaligned_offset_rejected() {
    let mut bytes = create_sample_package();

    // Set chunk offset to unaligned value (e.g. 65 instead of 64)
    modify_toc(&mut bytes, |raw_toc| {
        let unaligned_offset: u64 = 65;
        raw_toc[8..16].copy_from_slice(&unaligned_offset.to_le_bytes());
    });

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::UnalignedOffset { offset: 65 }) => {}
        Err(other) => panic!("Expected UnalignedOffset, got {:?}", other),
        Ok(_) => panic!("Unaligned chunk offset must be rejected"),
    }
}

#[test]
fn test_chunk_count_mismatch_rejected() {
    let mut bytes = create_sample_package();

    // Modify header chunk_count from 1 to 5
    bytes[56..60].copy_from_slice(&5u32.to_le_bytes());
    let crc = crc32fast::hash(&bytes[0..60]);
    bytes[60..64].copy_from_slice(&crc.to_le_bytes());

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::InvalidToc(msg)) => {
            assert!(msg.contains("Chunk count mismatch"));
        }
        Err(other) => panic!(
            "Expected InvalidToc for chunk_count mismatch, got {:?}",
            other
        ),
        Ok(_) => panic!("Header chunk_count mismatch must be rejected"),
    }
}

#[test]
fn test_toc_entry_offset_past_eof_rejected() {
    let mut bytes = create_sample_package();

    // Set Entry 0 offset to past EOF
    modify_toc(&mut bytes, |raw_toc| {
        let past_eof_offset: u64 = 100_000_000;
        raw_toc[8..16].copy_from_slice(&past_eof_offset.to_le_bytes());
    });

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::OutOfBounds { .. }) => {}
        Err(other) => panic!("Expected OutOfBounds, got {:?}", other),
        Ok(_) => panic!("Package with chunk offset past EOF must be rejected"),
    }
}

#[test]
fn test_toc_entry_overlapping_chunks_rejected() {
    let mut bytes = create_two_chunk_package();

    // Set chunk B offset equal to chunk A offset so they overlap
    modify_toc(&mut bytes, |raw_toc| {
        let chunk_a_offset = u64::from_le_bytes(raw_toc[8..16].try_into().unwrap());
        let id_a_len = u16::from_le_bytes(raw_toc[48..50].try_into().unwrap()) as usize;
        let entry_b_start = 50 + id_a_len;
        // Entry B offset is at entry_b_start + 4 .. entry_b_start + 12
        raw_toc[entry_b_start + 4..entry_b_start + 12]
            .copy_from_slice(&chunk_a_offset.to_le_bytes());
    });

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::InvalidToc(msg)) => {
            assert!(
                msg.contains("Overlapping"),
                "Error message should mention overlapping: {msg}"
            );
        }
        Err(other) => panic!(
            "Expected InvalidToc for overlapping chunks, got {:?}",
            other
        ),
        Ok(_) => panic!("Package with overlapping chunks must be rejected"),
    }
}

#[test]
fn test_toc_entry_duplicate_id_rejected() {
    let mut bytes = create_two_chunk_package();

    // Set chunk B identifier to be identical to chunk A identifier
    modify_toc(&mut bytes, |raw_toc| {
        let id_a_len = u16::from_le_bytes(raw_toc[48..50].try_into().unwrap()) as usize;
        let id_a_bytes = raw_toc[50..50 + id_a_len].to_vec();

        let entry_b_start = 50 + id_a_len;
        let id_b_len_offset = entry_b_start + 44;

        let mut new_toc = raw_toc[..id_b_len_offset].to_vec();
        new_toc.extend_from_slice(&(id_a_len as u16).to_le_bytes());
        new_toc.extend_from_slice(&id_a_bytes);

        // Replace raw_toc
        *raw_toc = new_toc;
    });

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::InvalidToc(msg)) => {
            assert!(
                msg.contains("Duplicate"),
                "Error message should mention duplicate: {msg}"
            );
        }
        Err(other) => panic!("Expected InvalidToc for duplicate id, got {:?}", other),
        Ok(_) => panic!("Package with duplicate chunk identifiers must be rejected"),
    }
}

#[test]
fn test_newer_minor_version_rejected() {
    let mut bytes = create_sample_package();

    let minor = u16::from_le_bytes([bytes[6], bytes[7]]);
    bytes[6..8].copy_from_slice(&(minor + 1).to_le_bytes());
    let crc = crc32fast::hash(&bytes[0..60]);
    bytes[60..64].copy_from_slice(&crc.to_le_bytes());

    let res = MtkPackageReader::from_bytes(bytes);
    match res {
        Err(PackageError::UnsupportedVersion { minor: found, .. }) => {
            assert_eq!(found, minor + 1);
        }
        Err(other) => panic!("Expected UnsupportedVersion, got {:?}", other),
        Ok(_) => panic!("Package with newer minor version must be rejected"),
    }
}

#[test]
fn test_extract_chunk_to_file_path_traversal_rejected() {
    let bytes = create_sample_package();
    let reader = MtkPackageReader::from_bytes(bytes).unwrap();

    let temp = tempdir().unwrap();
    let escaped_path = temp.path().join("sub").join("..").join("escaped.txt");

    // Identifier containing ".."
    let res = reader.extract_chunk_to_file("../../escape", &escaped_path);
    assert!(
        res.is_err(),
        "extract_chunk_to_file with '../../escape' must return Err"
    );
    assert!(
        !escaped_path.exists(),
        "File must not be created when path traversal is detected"
    );

    // Absolute identifier
    let res = reader.extract_chunk_to_file("/absolute/path", &escaped_path);
    assert!(
        res.is_err(),
        "extract_chunk_to_file with absolute path must return Err"
    );
    assert!(
        !escaped_path.exists(),
        "File must not be created for absolute chunk id"
    );

    // Empty identifier
    let res = reader.extract_chunk_to_file("", &escaped_path);
    assert!(
        res.is_err(),
        "extract_chunk_to_file with empty identifier must return Err"
    );
    assert!(
        !escaped_path.exists(),
        "File must not be created for empty chunk id"
    );
}
