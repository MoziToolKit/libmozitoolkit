use mtk_voxel::{SectionStorage, SparseVoxelColumns, SparseVoxelPayload, VoxelPointCloud};

#[test]
fn test_section_palette_index_out_of_range_rejects() {
    // 4096-element voxel array where index 0 points to palette entry 5,
    // but the palette only contains 2 entries (0: air, 1: stone).
    let mut voxels = vec![0u16; 4096];
    voxels[0] = 5;
    let voxels_json = serde_json::to_string(&voxels).unwrap();
    let json = format!(
        r#"{{
            "coord": [0, 0, 0],
            "palette": ["minecraft:air", "minecraft:stone"],
            "voxels": {},
            "non_air_count": 1
        }}"#,
        voxels_json
    );

    let res: Result<SectionStorage, _> = serde_json::from_str(&json);
    assert!(
        res.is_err(),
        "Deserialization must reject SectionStorage with out-of-range palette index"
    );
}

#[test]
fn test_section_empty_palette_rejects() {
    let voxels = vec![0u16; 4096];
    let voxels_json = serde_json::to_string(&voxels).unwrap();
    let json = format!(
        r#"{{
            "coord": [0, 0, 0],
            "palette": [],
            "voxels": {},
            "non_air_count": 0
        }}"#,
        voxels_json
    );

    let res: Result<SectionStorage, _> = serde_json::from_str(&json);
    assert!(
        res.is_err(),
        "Deserialization must reject SectionStorage with empty palette"
    );
}

#[test]
fn test_ragged_sparse_voxel_columns_rejects() {
    // SparseVoxelColumns where y (len 1) and z (len 2) are shorter than x (len 3).
    let ragged_columns_json = r#"{
        "x": [0, 1, 2],
        "y": [0],
        "z": [0, 1],
        "state_indices": [1, 1, 1],
        "biome_indices": [0, 0, 0],
        "entity_indices": [-1, -1, -1]
    }"#;

    let col_res: Result<SparseVoxelColumns, _> = serde_json::from_str(ragged_columns_json);
    assert!(
        col_res.is_err(),
        "Deserialization must reject ragged SparseVoxelColumns"
    );

    let ragged_payload_json = format!(
        r#"{{
            "bounds": [0, 0, 0, 16, 16, 16],
            "state_palette": ["minecraft:air", "minecraft:stone"],
            "biome_palette": ["minecraft:plains"],
            "entity_palette": [],
            "columns": {}
        }}"#,
        ragged_columns_json
    );

    let payload_res: Result<SparseVoxelPayload, _> = serde_json::from_str(&ragged_payload_json);
    assert!(
        payload_res.is_err(),
        "Deserialization must reject SparseVoxelPayload with ragged columns"
    );
}

#[test]
fn test_ragged_sparse_columns_to_point_cloud_no_panic() {
    // Even if constructed in memory bypassing serde, to_point_cloud must never panic.
    let payload = SparseVoxelPayload {
        bounds: [0, 0, 0, 16, 16, 16],
        state_palette: vec!["minecraft:air".to_string(), "minecraft:stone".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
        entity_palette: Vec::new(),
        columns: SparseVoxelColumns {
            x: vec![0, 1, 2, 3],
            y: vec![0],
            z: vec![0, 1],
            state_indices: vec![1, 1],
            biome_indices: vec![0, 0],
            entity_indices: vec![-1, -1],
            light_levels: None,
        },
    };

    let cloud = payload.to_point_cloud(None);
    assert_eq!(cloud.len(), 1);
}

#[test]
fn test_palette_index_negative_one_rejects_without_silent_zero() {
    // Serialized VoxelPointCloud with state_indices containing -1 (sentinel).
    // Previously `i.max(0) as u16` silently mapped -1 to 0. It must now return Err.
    let json = r#"{
        "positions": [0.5, 0.5, 0.5],
        "block_x": [0],
        "block_y": [0],
        "block_z": [0],
        "state_palette": ["minecraft:air", "minecraft:stone"],
        "state_indices": [-1],
        "biome_palette": ["minecraft:plains"],
        "biome_indices": [0],
        "entity_palette": [],
        "entity_indices": [-1],
        "light_levels": []
    }"#;

    let cloud: VoxelPointCloud =
        serde_json::from_str(json).expect("Hand-serialized VoxelPointCloud with state index -1");
    let res = SparseVoxelPayload::from_point_cloud(&cloud);
    assert!(
        res.is_err(),
        "from_point_cloud must return Err for state index -1 rather than silently substituting index 0"
    );

    // Also check biome_indices containing -1
    let biome_neg_json = r#"{
        "positions": [0.5, 0.5, 0.5],
        "block_x": [0],
        "block_y": [0],
        "block_z": [0],
        "state_palette": ["minecraft:air", "minecraft:stone"],
        "state_indices": [0],
        "biome_palette": ["minecraft:plains"],
        "biome_indices": [-1],
        "entity_palette": [],
        "entity_indices": [-1],
        "light_levels": []
    }"#;
    let biome_cloud: VoxelPointCloud =
        serde_json::from_str(biome_neg_json).expect("VoxelPointCloud with biome index -1");
    let biome_res = SparseVoxelPayload::from_point_cloud(&biome_cloud);
    assert!(
        biome_res.is_err(),
        "from_point_cloud must return Err for biome index -1 rather than silently substituting index 0"
    );
}

#[test]
fn test_palette_index_u16_overflow_rejects_without_silent_zero() {
    // Index 65536 cannot fit in u16. Previously `65536 as u16` truncated to 0.
    let json = r#"{
        "positions": [0.5, 0.5, 0.5],
        "block_x": [0],
        "block_y": [0],
        "block_z": [0],
        "state_palette": ["minecraft:air", "minecraft:stone"],
        "state_indices": [65536],
        "biome_palette": ["minecraft:plains"],
        "biome_indices": [0],
        "entity_palette": [],
        "entity_indices": [-1],
        "light_levels": []
    }"#;

    let cloud: VoxelPointCloud =
        serde_json::from_str(json).expect("Hand-serialized VoxelPointCloud with state index 65536");
    let res = SparseVoxelPayload::from_point_cloud(&cloud);
    assert!(
        res.is_err(),
        "from_point_cloud must return Err for index 65536 rather than truncating to index 0"
    );
}

#[test]
fn test_palette_count_overflow_rejects() {
    // State palette count > 65536 cannot be represented by u16 indices.
    let big_palette: Vec<String> = (0..65537)
        .map(|i| format!("minecraft:block_{}", i))
        .collect();

    let cloud = VoxelPointCloud {
        positions: vec![0.5, 0.5, 0.5],
        block_x: vec![0],
        block_y: vec![0],
        block_z: vec![0],
        state_palette: big_palette,
        state_indices: vec![0],
        biome_palette: vec!["minecraft:plains".to_string()],
        biome_indices: vec![0],
        entity_palette: vec![],
        entity_indices: vec![-1],
        light_levels: vec![],
        bounds: None,
    };

    let res = SparseVoxelPayload::from_point_cloud(&cloud);
    assert!(
        res.is_err(),
        "from_point_cloud must return Err when state_palette.len() > 65536"
    );
}

#[test]
fn test_sparse_payload_json_out_of_range_state_index_rejects() {
    // Hand-crafted JSON for SparseVoxelPayload where state_indices points out-of-range (index 5, palette len 2)
    let json = r#"{
        "bounds": [0, 0, 0, 16, 16, 16],
        "state_palette": ["minecraft:air", "minecraft:stone"],
        "biome_palette": ["minecraft:plains"],
        "entity_palette": [],
        "columns": {
            "x": [0],
            "y": [0],
            "z": [0],
            "state_indices": [5],
            "biome_indices": [0],
            "entity_indices": [-1]
        }
    }"#;

    let res: Result<SparseVoxelPayload, _> = serde_json::from_str(json);
    assert!(
        res.is_err(),
        "Deserializing SparseVoxelPayload with out-of-range state index must return Err"
    );
}
