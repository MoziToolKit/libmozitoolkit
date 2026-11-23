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
fn test_palette_index_out_of_range_rejects_without_silent_zero() {
    // Index 65536 against a 2-entry palette must return Err rather than truncating.
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
        "from_point_cloud must return Err for out-of-range index 65536 rather than truncating to index 0"
    );
}

#[test]
fn test_large_state_palette_exceeding_u16_roundtrips() {
    const COUNT: usize = 69_932;
    let big_palette: Vec<String> = (0..COUNT).map(|i| format!("modded:block_{i}")).collect();
    let mut positions = Vec::with_capacity(COUNT * 3);
    let mut block_x = Vec::with_capacity(COUNT);
    let mut block_y = Vec::with_capacity(COUNT);
    let mut block_z = Vec::with_capacity(COUNT);
    let mut state_indices = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        positions.extend_from_slice(&[i as f32 + 0.5, 0.5, 0.5]);
        block_x.push(i as i32);
        block_y.push(0);
        block_z.push(0);
        state_indices.push(i as i32);
    }

    let cloud = VoxelPointCloud {
        positions,
        block_x,
        block_y,
        block_z,
        state_palette: big_palette,
        state_indices,
        biome_palette: vec!["minecraft:plains".to_string()],
        biome_indices: vec![0; COUNT],
        entity_palette: vec![],
        entity_indices: vec![-1; COUNT],
        light_levels: vec![],
        bounds: Some([0, 0, 0, COUNT as i32, 1, 1]),
    };

    let payload = SparseVoxelPayload::from_point_cloud(&cloud)
        .expect("from_point_cloud must support >65536 states via u32 palette indices");
    assert_eq!(payload.state_palette.len(), COUNT);
    assert_eq!(payload.columns.state_indices[0], 0);
    assert_eq!(payload.columns.state_indices[COUNT - 1], (COUNT - 1) as u32);

    let encoded = postcard::to_allocvec(&payload).expect("postcard serialize large payload");
    let decoded: SparseVoxelPayload =
        postcard::from_bytes(&encoded).expect("postcard deserialize large payload");
    assert_eq!(decoded.state_palette.len(), COUNT);
    assert_eq!(decoded.columns.state_indices[COUNT - 1], (COUNT - 1) as u32);

    let restored = decoded.to_point_cloud(None);
    assert_eq!(restored.len(), COUNT);
    assert_eq!(restored.state_at(0), Some("modded:block_0"));
    assert_eq!(restored.state_at(COUNT - 1), Some("modded:block_69931"));
}

#[test]
fn test_from_point_cloud_compacts_unused_and_duplicate_palettes() {
    let chest_json = r#"{"id":"minecraft:chest"}"#.to_string();
    let cloud = VoxelPointCloud {
        positions: vec![0.5, 0.5, 0.5, 1.5, 0.5, 0.5, 2.5, 0.5, 0.5],
        block_x: vec![0, 1, 2],
        block_y: vec![0, 0, 0],
        block_z: vec![0, 0, 0],
        state_palette: vec![
            "minecraft:air".to_string(),
            "minecraft:stone".to_string(),
            "minecraft:dirt".to_string(),
            "minecraft:stone".to_string(),
            "minecraft:oak_planks".to_string(),
        ],
        state_indices: vec![1, 4, 3],
        biome_palette: vec![
            "minecraft:desert".to_string(),
            "minecraft:plains".to_string(),
            "minecraft:plains".to_string(),
        ],
        biome_indices: vec![1, 2, 1],
        entity_palette: vec![
            r#"{"id":"minecraft:sign"}"#.to_string(),
            chest_json.clone(),
            r#"{"id":"minecraft:banner"}"#.to_string(),
            chest_json.clone(),
        ],
        entity_indices: vec![-1, 1, 3],
        light_levels: vec![],
        bounds: Some([0, 0, 0, 3, 1, 1]),
    };

    let payload = SparseVoxelPayload::from_point_cloud(&cloud).expect("compact palettes");
    assert_eq!(
        payload.state_palette,
        vec![
            "minecraft:stone".to_string(),
            "minecraft:oak_planks".to_string(),
        ]
    );
    assert_eq!(payload.columns.state_indices, vec![0, 1, 0]);
    assert_eq!(payload.biome_palette, vec!["minecraft:plains".to_string()]);
    assert_eq!(payload.columns.biome_indices, vec![0, 0, 0]);
    assert_eq!(payload.entity_palette, vec![chest_json.clone()]);
    assert_eq!(payload.columns.entity_indices, vec![-1, 0, 0]);

    let restored = payload.to_point_cloud(None);
    assert_eq!(restored.state_at(0), Some("minecraft:stone"));
    assert_eq!(restored.state_at(1), Some("minecraft:oak_planks"));
    assert_eq!(restored.state_at(2), Some("minecraft:stone"));
    assert_eq!(restored.biome_at(0), Some("minecraft:plains"));
    assert_eq!(restored.biome_at(1), Some("minecraft:plains"));
    assert_eq!(restored.biome_at(2), Some("minecraft:plains"));
    assert_eq!(restored.entity_at(0), None);
    assert_eq!(restored.entity_at(1), Some(chest_json.as_str()));
    assert_eq!(restored.entity_at(2), Some(chest_json.as_str()));
}

#[test]
fn test_sparse_payload_json_index_above_i32_max_rejects() {
    let json = r#"{
        "bounds": [0, 0, 0, 16, 16, 16],
        "state_palette": ["minecraft:air", "minecraft:stone"],
        "biome_palette": ["minecraft:plains"],
        "entity_palette": [],
        "columns": {
            "x": [0],
            "y": [0],
            "z": [0],
            "state_indices": [2147483648],
            "biome_indices": [0],
            "entity_indices": [-1]
        }
    }"#;

    let res: Result<SparseVoxelPayload, _> = serde_json::from_str(json);
    assert!(
        res.is_err(),
        "Deserializing SparseVoxelPayload with state index > i32::MAX must return Err"
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
