use super::*;

#[test]
fn test_load_chest() {
    let bs_north = BlockState::parse("minecraft:chest[facing=north,type=single]").unwrap();
    let model_north =
        MiExModelLoader::load_for_blockstate(&bs_north).expect("Chest north should load");
    let elems_north = model_north.elements.unwrap_or_default();
    assert!(!elems_north.is_empty());
    let t_north = elems_north[0]
        .transform
        .as_ref()
        .expect("Transform should be present");
    assert_eq!(t_north.rotate[1], 0.0);

    let bs_south = BlockState::parse("minecraft:chest[facing=south,type=single]").unwrap();
    let model_south =
        MiExModelLoader::load_for_blockstate(&bs_south).expect("Chest south should load");
    let elems_south = model_south.elements.unwrap_or_default();
    let t_south = elems_south[0]
        .transform
        .as_ref()
        .expect("Transform should be present");
    assert_eq!(
        t_south.rotate[1], 180.0,
        "South chest should rotate 180 deg"
    );

    let bs_east = BlockState::parse("minecraft:chest[facing=east,type=single]").unwrap();
    let model_east =
        MiExModelLoader::load_for_blockstate(&bs_east).expect("Chest east should load");
    let elems_east = model_east.elements.unwrap();
    let t_east = elems_east[0].transform.as_ref().unwrap();
    assert_eq!(t_east.rotate[1], 90.0, "East chest should rotate 90 deg");
}

#[test]
fn test_skull_floor_vs_wall() {
    let bs_floor = BlockState::parse("minecraft:skeleton_skull[rotation=0]").unwrap();
    let model_floor =
        MiExModelLoader::load_for_blockstate(&bs_floor).expect("Floor skull should load");
    let elems_floor = model_floor.elements.unwrap();
    assert_eq!(
        elems_floor[0].from,
        [4.0, 0.0, 4.0],
        "Floor skull must start at Y=0"
    );

    let bs_wall = BlockState::parse("minecraft:skeleton_wall_skull[facing=north]").unwrap();
    let model_wall =
        MiExModelLoader::load_for_blockstate(&bs_wall).expect("Wall skull should load");
    let elems_wall = model_wall.elements.unwrap();
    assert_eq!(
        elems_wall[0].from,
        [4.0, 4.0, 0.0],
        "Wall skull must start at Y=4, Z=0"
    );
}

#[test]
fn test_dragon_and_piglin_heads() {
    let bs_dragon = BlockState::parse("minecraft:dragon_head[rotation=0]").unwrap();
    let model_dragon =
        MiExModelLoader::load_for_blockstate(&bs_dragon).expect("Dragon head should load");
    let elems_dragon = model_dragon.elements.unwrap();
    assert_eq!(elems_dragon.len(), 7, "Dragon head should have 7 elements");
    let tex_dragon = model_dragon.textures.unwrap();
    assert_eq!(
        tex_dragon.get("texture").unwrap().as_str(),
        "minecraft:entity/enderdragon/dragon"
    );

    // Verify dragon jaw rotation (element 2)
    let jaw = &elems_dragon[2];
    let jaw_rot = jaw
        .rotation
        .as_ref()
        .expect("Dragon jaw should have rotation");
    assert_eq!(jaw_rot.axis, "x");
    assert_eq!(jaw_rot.angle, 11.5);
    assert_eq!(jaw_rot.origin, [8.0, 3.0, 14.0]);

    let bs_piglin = BlockState::parse("minecraft:piglin_head[rotation=4]").unwrap();
    let model_piglin =
        MiExModelLoader::load_for_blockstate(&bs_piglin).expect("Piglin head should load");
    let elems_piglin = model_piglin.elements.unwrap();
    assert_eq!(elems_piglin.len(), 6, "Piglin head should have 6 elements");
    let tex_piglin = model_piglin.textures.unwrap();
    assert_eq!(
        tex_piglin.get("texture").unwrap().as_str(),
        "minecraft:entity/piglin/piglin"
    );
    let t_piglin = elems_piglin[0].transform.as_ref().unwrap();
    assert_eq!(
        t_piglin.rotate[1], 270.0,
        "Rotation 4 should be 270 deg (22.5*4 + 180)"
    );

    // Verify piglin ear rotations and distinct UVs
    let right_ear = &elems_piglin[2];
    let right_ear_rot = right_ear
        .rotation
        .as_ref()
        .expect("Right ear should have rotation");
    assert_eq!(right_ear_rot.axis, "z");
    assert_eq!(right_ear_rot.angle, 30.0);
    assert_eq!(right_ear_rot.origin, [12.5, 6.0, 8.0]);

    let left_ear = &elems_piglin[3];
    let left_ear_rot = left_ear
        .rotation
        .as_ref()
        .expect("Left ear should have rotation");
    assert_eq!(left_ear_rot.axis, "z");
    assert_eq!(left_ear_rot.angle, -30.0);
    assert_eq!(left_ear_rot.origin, [3.5, 6.0, 8.0]);

    // Verify left and right ears have different face UVs
    let right_faces = &right_ear.faces;
    let left_faces = &left_ear.faces;
    assert_ne!(
        right_faces.get("west").unwrap().uv,
        left_faces.get("west").unwrap().uv
    );
}

#[test]
fn test_hanging_sign_load() {
    let bs_standing =
        BlockState::parse("minecraft:oak_hanging_sign[attached=false,rotation=0]").unwrap();
    let model_standing = MiExModelLoader::load_for_blockstate(&bs_standing)
        .expect("Standing hanging sign must load");
    let elems_standing = model_standing.elements.unwrap_or_default();
    // Board (1) + 4 angled chains (4) = 5 elements. Absolutely NO vertical sign post!
    assert_eq!(
        elems_standing.len(),
        5,
        "Standing hanging sign without attachment must have board and 4 chains"
    );
    let tex = model_standing.textures.unwrap();
    assert_eq!(
        tex.get("texture").unwrap().as_str(),
        "minecraft:entity/signs/hanging/oak"
    );
    for elem in &elems_standing {
        // Confirm none of the elements are the vertical sign post (which had X in [7, 9], Y in [0, 9.333])
        let is_post =
            elem.from[0] > 6.0 && elem.to[0] < 10.0 && elem.from[1] < 1.0 && elem.to[1] < 10.0;
        assert!(
            !is_post,
            "Hanging sign must NOT contain vertical standing sign post!"
        );
    }

    let bs_wall = BlockState::parse("minecraft:oak_wall_hanging_sign[facing=north]").unwrap();
    let model_wall =
        MiExModelLoader::load_for_blockstate(&bs_wall).expect("Wall hanging sign must load");
    let elems_wall = model_wall.elements.unwrap_or_default();
    for (i, el) in elems_wall.iter().enumerate() {
        println!("Wall elem {}: from={:?}, to={:?}", i, el.from, el.to);
    }
    // Board (1) + wall bracket (1) = 2 elements
    assert_eq!(
        elems_wall.len(),
        2,
        "Wall hanging sign must have board and bracket"
    );

    let bs_attached =
        BlockState::parse("minecraft:oak_hanging_sign[attached=true,rotation=0]").unwrap();
    let model_attached = MiExModelLoader::load_for_blockstate(&bs_attached)
        .expect("Attached hanging sign must load");
    let elems_attached = model_attached.elements.unwrap_or_default();
    // Board (1) + center chain (1) = 2 elements
    assert_eq!(
        elems_attached.len(),
        2,
        "Attached hanging sign must have board and center chain"
    );
}

#[test]
fn test_unprofiled_skull_direct_atlas_textures() {
    use crate::entity::skull::SkullKind;
    use crate::entity::BlockEntityData;

    // 1. Zombie head without profile -> direct internal zombie texture
    let bs_zombie = BlockState::parse("minecraft:zombie_head[rotation=0]").unwrap();
    let entity_zombie = BlockEntityData::Skull {
        kind: SkullKind::Zombie,
        rotation: Some(0),
        profile: None,
    };
    let model_zombie =
        MiExModelLoader::load_for_blockstate_with_entity(&bs_zombie, Some(&entity_zombie))
            .expect("Zombie head should load");
    let tex_zombie = model_zombie.textures.unwrap();
    assert_eq!(
        tex_zombie.get("texture").unwrap().as_str(),
        "minecraft:entity/zombie/zombie"
    );

    // 2. Creeper wall head without profile -> direct internal creeper texture
    let bs_creeper = BlockState::parse("minecraft:creeper_wall_head[facing=north]").unwrap();
    let entity_creeper = BlockEntityData::Skull {
        kind: SkullKind::Creeper,
        rotation: None,
        profile: None,
    };
    let model_creeper =
        MiExModelLoader::load_for_blockstate_with_entity(&bs_creeper, Some(&entity_creeper))
            .expect("Creeper head should load");
    let tex_creeper = model_creeper.textures.unwrap();
    assert_eq!(
        tex_creeper.get("texture").unwrap().as_str(),
        "minecraft:entity/creeper/creeper"
    );

    // 3. Player head without profile -> direct internal default steve texture
    let bs_player = BlockState::parse("minecraft:player_head[rotation=4]").unwrap();
    let entity_player = BlockEntityData::Skull {
        kind: SkullKind::Player,
        rotation: Some(4),
        profile: None,
    };
    let model_player =
        MiExModelLoader::load_for_blockstate_with_entity(&bs_player, Some(&entity_player))
            .expect("Player head without profile should load");
    let tex_player = model_player.textures.unwrap();
    assert_eq!(
        tex_player.get("texture").unwrap().as_str(),
        "minecraft:entity/player/wide/steve"
    );
}
