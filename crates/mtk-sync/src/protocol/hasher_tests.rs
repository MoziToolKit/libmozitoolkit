    use super::*;

    #[test]
    fn test_canonical_hash_deterministic() {
        let min_pos = IVec3::new(0, 64, 0);
        let size = IVec3::new(2, 2, 2);

        let h1 = compute_canonical_state_hash(
            min_pos,
            size,
            "minecraft:overworld",
            |_, _, _| "minecraft:stone".to_string(),
            |_, _, _| "minecraft:plains".to_string(),
        );

        let h2 = compute_canonical_state_hash(
            min_pos,
            size,
            "minecraft:overworld",
            |_, _, _| "minecraft:stone".to_string(),
            |_, _, _| "minecraft:plains".to_string(),
        );

        assert_eq!(h1, h2);
        assert_ne!(h1, 0);
    }

    #[test]
    fn test_air_family_normalization_matches_java_ssot() {
        // Air family states collapse to canonical "minecraft:air"
        for raw in [
            "minecraft:air",
            "minecraft:cave_air",
            "minecraft:void_air",
            "minecraft:structure_void",
            "cave_air",
            "void_air",
            "structure_void",
        ] {
            assert_eq!(
                normalize_canonical_blockstate(raw),
                "minecraft:air",
                "raw state {raw} must canonicalise to air"
            );
        }

        // Authentic block states preserve their full identifier and property strings.
        for raw in [
            "minecraft:bubble_column",
            "minecraft:bubble_column[drag=true]",
            "minecraft:bubble_column[drag=false]",
            "minecraft:light[level=15,waterlogged=true]",
            "minecraft:light[level=15,waterlogged=false]",
            "minecraft:barrier",
            "minecraft:barrier[waterlogged=true]",
            "minecraft:oak_stairs[facing=north]",
        ] {
            assert_eq!(
                normalize_canonical_blockstate(raw),
                raw,
                "raw state {raw} must be preserved authentically"
            );
        }
    }

    #[test]
    fn test_sparse_state_hash_storage_matches_point_cloud() {
        use mtk_voxel::{MesherConfig, SectionStorage, VoxelPointCloud, VoxelStorage};

        let mut storage = VoxelStorage::new();
        let min_pos = IVec3::new(0, 64, 0);
        let size = IVec3::new(16, 16, 16);

        let sec_coord = IVec3::new(0, 4, 0); // y=64..79
        let mut sec = SectionStorage::new(sec_coord);
        sec.set_local(0, 0, 0, "minecraft:stone");
        sec.set_local(5, 2, 7, "minecraft:oak_stairs[facing=north]");
        sec.set_local(15, 15, 15, "minecraft:dirt");
        sec.set_local(1, 1, 1, "minecraft:cave_air"); // Air should be skipped
        storage.sections.insert(sec_coord, sec);

        let hash_storage =
            compute_sparse_state_hash_for_storage(&storage, min_pos, size, "minecraft:overworld");
        assert_ne!(hash_storage, 0);

        // Extract point cloud from storage
        let cloud = VoxelPointCloud::extract_from_storage(&storage, &MesherConfig::default());
        assert_eq!(cloud.len(), 3); // exactly 3 non-air blocks

        let hash_cloud =
            compute_sparse_state_hash_for_point_cloud(&cloud, min_pos, size, "minecraft:overworld");

        // Point cloud and storage must produce the exact same sparse hash!
        assert_eq!(
            hash_storage, hash_cloud,
            "sparse hash from VoxelStorage and VoxelPointCloud must be identical"
        );

        // Now test shuffling the point cloud order: hash must still be identical!
        let mut shuffled_cloud = cloud.clone();
        // swap point 0 and point 2
        shuffled_cloud.block_x.swap(0, 2);
        shuffled_cloud.block_y.swap(0, 2);
        shuffled_cloud.block_z.swap(0, 2);
        shuffled_cloud.state_indices.swap(0, 2);
        shuffled_cloud.biome_indices.swap(0, 2);
        if !shuffled_cloud.entity_indices.is_empty() {
            shuffled_cloud.entity_indices.swap(0, 2);
        }

        let hash_shuffled = compute_sparse_state_hash_for_point_cloud(
            &shuffled_cloud,
            min_pos,
            size,
            "minecraft:overworld",
        );
        assert_eq!(
            hash_storage, hash_shuffled,
            "shuffled point cloud must produce the exact same canonical sparse hash"
        );

        // Modifying any block must change the hash!
        let mut modified_cloud = cloud.clone();
        modified_cloud
            .state_palette
            .push("minecraft:gold_block".to_string());
        modified_cloud.state_indices[0] = (modified_cloud.state_palette.len() - 1) as i32;
        let hash_modified = compute_sparse_state_hash_for_point_cloud(
            &modified_cloud,
            min_pos,
            size,
            "minecraft:overworld",
        );
        assert_ne!(
            hash_storage, hash_modified,
            "modifying block must alter the hash"
        );

        // Deleting a block must change the hash!
        let mut deleted_cloud = cloud.clone();
        deleted_cloud.block_x.pop();
        deleted_cloud.block_y.pop();
        deleted_cloud.block_z.pop();
        deleted_cloud.state_indices.pop();
        deleted_cloud.biome_indices.pop();
        let hash_deleted = compute_sparse_state_hash_for_point_cloud(
            &deleted_cloud,
            min_pos,
            size,
            "minecraft:overworld",
        );
        assert_ne!(
            hash_storage, hash_deleted,
            "deleting block must alter the hash"
        );
    }

    #[test]
    fn test_reference_sparse_state_hash() {
        let min_pos = IVec3::new(0, 64, 0);
        let size = IVec3::new(2, 2, 2);

        let entries = vec![
            SparseBlockEntry {
                rel_x: 0,
                rel_y: 0,
                rel_z: 0,
                state: "minecraft:stone",
                biome: "minecraft:plains",
                entity_json: None,
            },
            SparseBlockEntry {
                rel_x: 1,
                rel_y: 1,
                rel_z: 1,
                state: "minecraft:oak_stairs[facing=north]",
                biome: "minecraft:plains",
                entity_json: None,
            },
        ];

        let hash = compute_sparse_state_hash(min_pos, size, "minecraft:overworld", entries);
        assert_ne!(hash, 0);
        // We will lock this down as reference value across Rust and Java
        assert_eq!(hash, 7_416_261_855_900_228_840u64);
    }
