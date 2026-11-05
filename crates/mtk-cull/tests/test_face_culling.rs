use glam::IVec3;
use mtk_core::Aabb2d;
use mtk_core::Direction;
use mtk_cull::{CullCategory, FaceCuller, GlassCullMode, LeavesCullMode};

#[test]
fn test_solid_opaque_mutual_culling() {
    let culler = FaceCuller::default();
    let stone = culler.get_meta("minecraft:stone", None, None);
    let dirt = culler.get_meta("minecraft:dirt", None, None);
    let air = culler.get_meta("minecraft:air", None, None);

    assert_eq!(stone.category, CullCategory::SolidOpaque);
    assert!(stone.has_full_face(Direction::East));

    // Stone touching Dirt on East (+X): Stone East face should be culled
    assert!(!culler.should_render_face(&stone, Some(&dirt), Direction::East, None, None, None));
    // Dirt touching Stone on West (-X): Dirt West face should be culled
    assert!(!culler.should_render_face(&dirt, Some(&stone), Direction::West, None, None, None));

    // Stone touching Air: Stone face should be rendered
    assert!(culler.should_render_face(&stone, Some(&air), Direction::East, None, None, None));
    assert!(culler.should_render_face(&stone, None, Direction::East, None, None, None));
}

#[test]
fn test_glass_translucent_culling() {
    let mut culler = FaceCuller::default();
    let glass = culler.get_meta("minecraft:glass", None, None);
    let red_glass = culler.get_meta("minecraft:red_stained_glass", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);

    assert_eq!(glass.category, CullCategory::GlassTranslucent);

    // 1. Glass touching Glass: mutually culled
    assert!(!culler.should_render_face(&glass, Some(&glass), Direction::East, None, None, None));
    assert!(!culler.should_render_face(&glass, Some(&glass), Direction::West, None, None, None));

    // 2. In GROUP mode: Red Stained Glass touching Plain Glass -> culled
    culler.glass_cull_mode = GlassCullMode::Group;
    assert!(!culler.should_render_face(
        &red_glass,
        Some(&glass),
        Direction::East,
        None,
        None,
        None
    ));

    // 3. In SAME_BLOCK mode: Red Stained Glass touching Plain Glass -> rendered partition
    culler.glass_cull_mode = GlassCullMode::SameBlock;
    assert!(culler.should_render_face(&red_glass, Some(&glass), Direction::East, None, None, None));

    // 4. Glass touching Stone:
    // Glass touching Stone: Stone has full occlusion -> Glass culls its own face against Stone
    assert!(!culler.should_render_face(&glass, Some(&stone), Direction::East, None, None, None));
    // Stone touching Glass: Glass has empty occlusion shape -> Stone RENDERS its face against Glass
    assert!(culler.should_render_face(&stone, Some(&glass), Direction::West, None, None, None));
}

#[test]
fn test_cutout_leaves_modes() {
    let mut culler = FaceCuller::default();
    let oak_leaves = culler.get_meta("minecraft:oak_leaves", None, None);
    let birch_leaves = culler.get_meta("minecraft:birch_leaves", None, None);
    let oak_log = culler.get_meta("minecraft:oak_log", None, None);

    assert_eq!(oak_leaves.category, CullCategory::CutoutLeaves);
    assert_eq!(culler.leaves_cull_mode, LeavesCullMode::SingleFace);

    // 1. Fancy Mode: both leaves faces rendered (internal volume visible)
    culler.leaves_cull_mode = LeavesCullMode::Fancy;
    assert!(culler.should_render_face(
        &oak_leaves,
        Some(&birch_leaves),
        Direction::East,
        None,
        None,
        None
    ));
    assert!(culler.should_render_face(
        &birch_leaves,
        Some(&oak_leaves),
        Direction::West,
        None,
        None,
        None
    ));

    // 2. Single-Face Mode: exactly one face rendered between touching leaves
    culler.leaves_cull_mode = LeavesCullMode::SingleFace;
    let pos_a = IVec3::new(0, 0, 0);
    let pos_b = IVec3::new(1, 0, 0);
    let render_a = culler.should_render_face(
        &oak_leaves,
        Some(&oak_leaves),
        Direction::East,
        None,
        Some(pos_a),
        Some(pos_b),
    );
    let render_b = culler.should_render_face(
        &oak_leaves,
        Some(&oak_leaves),
        Direction::West,
        None,
        Some(pos_b),
        Some(pos_a),
    );
    // Exactly one of them should be true and the other false
    assert_ne!(render_a, render_b);
    assert!(render_a || render_b);

    // 3. Fast Mode: mutually culled
    culler.leaves_cull_mode = LeavesCullMode::Fast;
    assert!(!culler.should_render_face(
        &oak_leaves,
        Some(&birch_leaves),
        Direction::East,
        None,
        None,
        None
    ));
    assert!(!culler.should_render_face(
        &birch_leaves,
        Some(&oak_leaves),
        Direction::West,
        None,
        None,
        None
    ));

    // 4. Leaves touching Solid Log:
    // Leaf face against log -> culled (log has full occlusion)
    assert!(!culler.should_render_face(
        &oak_leaves,
        Some(&oak_log),
        Direction::Down,
        None,
        None,
        None
    ));
    // Log face against leaf -> rendered
    assert!(culler.should_render_face(
        &oak_log,
        Some(&oak_leaves),
        Direction::Up,
        None,
        None,
        None
    ));
}

#[test]
fn test_partial_shape_slab_occlusion() {
    let culler = FaceCuller::default();
    let bottom_slab = culler.get_meta("minecraft:oak_slab[type=bottom]", None, None);
    let top_slab = culler.get_meta("minecraft:oak_slab[type=top]", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);

    // Bottom slab on Stone (down direction) -> Stone has full face -> Bottom slab down face is CULLED
    assert!(!culler.should_render_face(
        &bottom_slab,
        Some(&stone),
        Direction::Down,
        None,
        None,
        None
    ));
    // Stone placed above bottom slab (up direction) -> bottom slab up is empty -> Stone RENDERS down face
    assert!(culler.should_render_face(
        &stone,
        Some(&bottom_slab),
        Direction::Down,
        None,
        None,
        None
    ));

    // Top slab below Stone (up direction) -> Stone has full face -> Top slab up face is CULLED
    assert!(!culler.should_render_face(&top_slab, Some(&stone), Direction::Up, None, None, None));
}

#[test]
fn test_fluid_culling() {
    let culler = FaceCuller::default();
    let water = culler.get_meta("minecraft:water", None, None);
    let lava = culler.get_meta("minecraft:lava", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);

    assert_eq!(water.category, CullCategory::Fluid);

    // Water touching Water: culled
    assert!(!culler.should_render_face(&water, Some(&water), Direction::East, None, None, None));
    assert!(!culler.should_render_face(&water, Some(&water), Direction::Up, None, None, None));
    // Water touching Lava: rendered (different fluid)
    assert!(culler.should_render_face(&water, Some(&lava), Direction::East, None, None, None));

    // Water under Stone ceiling: the canonical 8/9 surface does not touch the ceiling,
    // so the top face must stay visible (vanilla FluidRenderer: height < 1.0).
    assert!(culler.should_render_face(&water, Some(&stone), Direction::Up, None, None, None));
    // Stone bottom face facing water: rendered
    assert!(culler.should_render_face(&stone, Some(&water), Direction::Down, None, None, None));

    // Water bottom face touching stone floor: culled
    assert!(!culler.should_render_face(&water, Some(&stone), Direction::Down, None, None, None));
    // Water side face touching stone wall: culled
    assert!(!culler.should_render_face(&water, Some(&stone), Direction::East, None, None, None));

    // Water against waterlogged block above: culled
    let waterlogged_slab = culler.get_meta(
        "minecraft:oak_slab[type=bottom,waterlogged=true]",
        None,
        None,
    );
    assert!(!culler.should_render_face(
        &water,
        Some(&waterlogged_slab),
        Direction::Up,
        None,
        None,
        None
    ));
}

#[test]
fn test_quad_level_element_culling() {
    let culler = FaceCuller::default();
    let stone = culler.get_meta("minecraft:stone", None, None);
    let slab_top = culler.get_meta("minecraft:oak_slab[type=top]", None, None);
    let air = culler.get_meta("minecraft:air", None, None);

    // A partial element quad on bottom boundary
    let partial_bottom_quad = Aabb2d::from_min_max(0.0, 0.0, 1.0, 0.5);

    // When touching a solid stone below (down direction), the partial quad is completely occluded by stone's full top face
    assert!(!culler.should_render_face(
        &slab_top,
        Some(&stone),
        Direction::Down,
        Some(&[partial_bottom_quad]),
        None,
        None
    ));

    // When touching an empty air below, the partial quad is visible
    assert!(culler.should_render_face(
        &slab_top,
        Some(&air),
        Direction::Down,
        Some(&[partial_bottom_quad]),
        None,
        None
    ));
}

#[test]
fn test_non_full_blocks_do_not_cull_glass_or_solid_faces() {
    let culler = FaceCuller::default();
    let glass = culler.get_meta("minecraft:glass", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);

    let non_full_blocks = [
        "minecraft:oak_fence",
        "minecraft:spruce_fence_gate",
        "minecraft:glass_pane",
        "minecraft:white_stained_glass_pane",
        "minecraft:red_stained_glass_pane",
        "minecraft:iron_bars",
        "minecraft:cobblestone_wall",
        "minecraft:stone_brick_wall",
        "minecraft:oak_trapdoor",
        "minecraft:iron_trapdoor",
        "minecraft:oak_door",
        "minecraft:iron_door",
        "minecraft:white_carpet",
        "minecraft:moss_carpet",
        "minecraft:chest",
        "minecraft:trapped_chest",
        "minecraft:ender_chest",
        "minecraft:torch",
        "minecraft:lantern",
        "minecraft:soul_lantern",
        "minecraft:chain",
        "minecraft:lightning_rod",
        "minecraft:end_rod",
        "minecraft:flower_pot",
        "minecraft:conduit",
        "minecraft:bell",
        "minecraft:anvil",
        "minecraft:cauldron",
        "minecraft:hopper",
        "minecraft:brewing_stand",
        "minecraft:scaffolding",
        "minecraft:pointed_dripstone",
    ];

    for block_name in non_full_blocks {
        let n_meta = culler.get_meta(block_name, None, None);

        // 1. Non-full block placed on top of Glass -> Glass UP face MUST render
        assert!(
            culler.should_render_face(&glass, Some(&n_meta), Direction::Up, None, None, None),
            "Glass top face was erroneously culled under non-full block: {}",
            block_name
        );

        // 2. Non-full block placed on top of Stone -> Stone UP face MUST render
        assert!(
            culler.should_render_face(&stone, Some(&n_meta), Direction::Up, None, None, None),
            "Stone top face was erroneously culled under non-full block: {}",
            block_name
        );

        // 3. Non-full block placed to the side (East) of Glass -> Glass East face MUST render
        assert!(
            culler.should_render_face(&glass, Some(&n_meta), Direction::East, None, None, None),
            "Glass east face was erroneously culled next to non-full block: {}",
            block_name
        );

        // 4. Non-full block placed to the side (East) of Stone -> Stone East face MUST render
        assert!(
            culler.should_render_face(&stone, Some(&n_meta), Direction::East, None, None, None),
            "Stone east face was erroneously culled next to non-full block: {}",
            block_name
        );
    }
}

#[test]
fn test_glass_pane_and_stained_glass_pane_do_not_skip_rendering_with_glass_block() {
    let culler = FaceCuller::default();
    let glass = culler.get_meta("minecraft:glass", None, None);
    let red_glass = culler.get_meta("minecraft:red_stained_glass", None, None);
    let pane = culler.get_meta("minecraft:glass_pane", None, None);
    let red_pane = culler.get_meta("minecraft:red_stained_glass_pane", None, None);
    let white_pane = culler.get_meta("minecraft:white_stained_glass_pane", None, None);

    // Glass against glass pane in any direction must render
    assert!(culler.should_render_face(&glass, Some(&pane), Direction::Up, None, None, None));
    assert!(culler.should_render_face(&glass, Some(&red_pane), Direction::Up, None, None, None));
    assert!(culler.should_render_face(&glass, Some(&white_pane), Direction::Up, None, None, None));
    assert!(culler.should_render_face(&red_glass, Some(&pane), Direction::East, None, None, None));
    assert!(culler.should_render_face(
        &red_glass,
        Some(&red_pane),
        Direction::East,
        None,
        None,
        None
    ));
}

#[test]
fn test_double_slab_and_stairs_culling() {
    let culler = FaceCuller::default();
    let stone = culler.get_meta("minecraft:stone", None, None);
    let glass = culler.get_meta("minecraft:glass", None, None);
    let double_slab = culler.get_meta("minecraft:oak_slab[type=double]", None, None);
    let stairs_bottom =
        culler.get_meta("minecraft:oak_stairs[facing=north,half=bottom]", None, None);
    let stairs_top = culler.get_meta("minecraft:oak_stairs[facing=north,half=top]", None, None);

    // Double slab is solid cube
    assert_eq!(double_slab.category, CullCategory::SolidOpaque);
    assert!(double_slab.has_full_face(Direction::Up));
    assert!(double_slab.has_full_face(Direction::Down));

    // Double slab touching Stone: mutually culled
    assert!(!culler.should_render_face(
        &double_slab,
        Some(&stone),
        Direction::East,
        None,
        None,
        None
    ));
    assert!(!culler.should_render_face(
        &stone,
        Some(&double_slab),
        Direction::West,
        None,
        None,
        None
    ));

    // Double slab above Glass: Glass top face is culled (double slab bottom is full solid)
    assert!(!culler.should_render_face(
        &glass,
        Some(&double_slab),
        Direction::Up,
        None,
        None,
        None
    ));

    // Bottom stairs above Stone: stairs bottom is full solid, so Stone top face is culled
    assert!(!culler.should_render_face(
        &stone,
        Some(&stairs_bottom),
        Direction::Up,
        None,
        None,
        None
    ));

    // Top stairs above Stone: stairs bottom is not full, so Stone top face MUST render
    assert!(culler.should_render_face(&stone, Some(&stairs_top), Direction::Up, None, None, None));
}

#[test]
fn test_snow_layers_mutual_culling_at_same_height() {
    let culler = FaceCuller::default();
    let snow_a = culler.get_meta("minecraft:snow[layers=2]", None, None);
    let snow_b = culler.get_meta("minecraft:snow[layers=2]", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);

    // Snow A is at (0, 0, 0), Snow B is at (1, 0, 0).
    // A's East face touches B's West face. Both have height = 2/8 (0.25).
    // Touching side faces MUST be culled to eliminate internal double faces and SSS dark seams.
    assert!(!culler.should_render_face(&snow_a, Some(&snow_b), Direction::East, None, None, None));
    assert!(!culler.should_render_face(&snow_b, Some(&snow_a), Direction::West, None, None, None));

    // Snow top face towards air MUST render
    assert!(culler.should_render_face(&snow_a, None, Direction::Up, None, None, None));

    // Snow placed on stone: Snow bottom face is culled by stone's full top face
    assert!(!culler.should_render_face(&snow_a, Some(&stone), Direction::Down, None, None, None));
    // Underlying stone top face is also culled to allow seamless manifold welding
    assert!(!culler.should_render_face(&stone, Some(&snow_a), Direction::Up, None, None, None));
}

#[test]
fn test_snow_layers_different_heights() {
    let culler = FaceCuller::default();
    let snow_short = culler.get_meta("minecraft:snow[layers=2]", None, None);
    let snow_tall = culler.get_meta("minecraft:snow[layers=5]", None, None);

    // Short snow touching tall snow: short snow side (0.25) is 100% covered by tall snow (0.625) -> CULLED
    assert!(!culler.should_render_face(
        &snow_short,
        Some(&snow_tall),
        Direction::East,
        None,
        None,
        None
    ));
    // Tall snow facing short snow: remaining upper portion (0.25 to 0.625) is not occluded -> RENDERS
    assert!(culler.should_render_face(
        &snow_tall,
        Some(&snow_short),
        Direction::West,
        None,
        None,
        None
    ));
}

#[test]
fn test_iron_bars_cross_section_culling() {
    let culler = FaceCuller::default();
    let bar_west = culler.get_meta(
        "minecraft:iron_bars[east=true,west=false,north=false,south=false]",
        None,
        None,
    );
    let bar_east = culler.get_meta(
        "minecraft:iron_bars[east=false,west=true,north=false,south=false]",
        None,
        None,
    );

    // Touching cross-sections on X boundary (bar_west East vs bar_east West) must be culled
    assert!(!culler.should_render_face(
        &bar_west,
        Some(&bar_east),
        Direction::East,
        None,
        None,
        None
    ));
    assert!(!culler.should_render_face(
        &bar_east,
        Some(&bar_west),
        Direction::West,
        None,
        None,
        None
    ));

    // Unconnected direction retains render state against air
    assert!(culler.should_render_face(&bar_west, None, Direction::North, None, None, None));
}

#[test]
fn test_fence_cross_section_culling() {
    let culler = FaceCuller::default();
    let fence_west = culler.get_meta(
        "minecraft:oak_fence[east=true,west=false,north=false,south=false]",
        None,
        None,
    );
    let fence_east = culler.get_meta(
        "minecraft:oak_fence[east=false,west=true,north=false,south=false]",
        None,
        None,
    );

    // Touching cross-sections on X boundary (fence_west East vs fence_east West) must be culled
    assert!(!culler.should_render_face(
        &fence_west,
        Some(&fence_east),
        Direction::East,
        None,
        None,
        None
    ));
    assert!(!culler.should_render_face(
        &fence_east,
        Some(&fence_west),
        Direction::West,
        None,
        None,
        None
    ));
}

#[test]
fn test_culler_cache_eviction() {
    let culler = FaceCuller::default();
    for i in 0..FaceCuller::CACHE_CAPACITY {
        culler.get_meta(&format!("dummy:state_{}", i), None, None);
    }
    assert_eq!(culler.cache_len(), FaceCuller::CACHE_CAPACITY);

    // Getting a new state should evict oldest
    culler.get_meta("minecraft:emerald_block", None, None);
    assert_eq!(culler.cache_len(), FaceCuller::CACHE_CAPACITY);
}

#[test]
fn test_vegetation_and_transparent_culling() {
    let culler = FaceCuller::default();

    // 1. Kelp and Seagrass (canonical inherently submerged vanilla blocks)
    let kelp = culler.get_meta("minecraft:kelp[age=0]", None, None);
    let dry_kelp = culler.get_meta("minecraft:kelp[age=0,waterlogged=false]", None, None);
    let seagrass = culler.get_meta("minecraft:seagrass", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);
    let water = culler.get_meta("minecraft:water[level=0]", None, None);

    assert_eq!(kelp.category, CullCategory::NonOccluding);
    assert_eq!(seagrass.category, CullCategory::NonOccluding);
    assert!(kelp.is_waterlogged);
    assert!(seagrass.is_waterlogged);
    assert!(!dry_kelp.is_waterlogged);
    assert!(!kelp.has_full_face(Direction::Up));
    assert!(!kelp.has_full_face(Direction::Down));

    // Stone underneath kelp (direction Up from stone towards kelp): stone top face must RENDER
    assert!(culler.should_render_face(&stone, Some(&kelp), Direction::Up, None, None, None));
    // Stone underneath seagrass: stone top face must RENDER
    assert!(culler.should_render_face(&stone, Some(&seagrass), Direction::Up, None, None, None));

    // Water against waterlogged kelp: water boundary face skips rendering (culls) to avoid inner split walls
    assert!(!culler.should_render_face(&water, Some(&kelp), Direction::Down, None, None, None));
    // Water against waterlogged seagrass: water boundary face skips rendering (culls)
    assert!(!culler.should_render_face(&water, Some(&seagrass), Direction::Down, None, None, None));
    // Water against dry kelp: water boundary face MUST render
    assert!(culler.should_render_face(&water, Some(&dry_kelp), Direction::Down, None, None, None));

    // 2. Leaf litter
    let leaf_litter = culler.get_meta("minecraft:leaf_litter", None, None);
    assert_eq!(leaf_litter.category, CullCategory::NonOccluding);
    let dirt = culler.get_meta("minecraft:dirt", None, None);
    // Dirt underneath leaf litter must render its top face
    assert!(culler.should_render_face(&dirt, Some(&leaf_litter), Direction::Up, None, None, None));

    // 3. Leaves touching solid log
    let leaves = culler.get_meta("minecraft:oak_leaves", None, None);
    let log = culler.get_meta("minecraft:oak_log[axis=y]", None, None);
    assert_eq!(leaves.category, CullCategory::CutoutLeaves);
    assert!(leaves.has_empty_face(Direction::West));
    // Log facing leaves on East: log face must RENDER
    assert!(culler.should_render_face(&log, Some(&leaves), Direction::East, None, None, None));
    // Leaves facing solid log on West: leaves face against solid wall is CULLED
    assert!(!culler.should_render_face(&leaves, Some(&log), Direction::West, None, None, None));

    // 4. JSON metadata from Yefira
    let json_plant = culler.get_meta(
        r#"{"state":"minecraft:custom_flower","type":1,"opaque":0}"#,
        None,
        None,
    );
    assert_eq!(json_plant.category, CullCategory::NonOccluding);
    assert!(!json_plant.is_opaque);
    assert!(!json_plant.has_full_face(Direction::Down));
    assert!(culler.should_render_face(&stone, Some(&json_plant), Direction::Up, None, None, None));

    // 5. Block with opaque=0 hint does not cull neighbor
    let transparent_cube = culler.get_meta("minecraft:translucent_cube", None, Some(false));
    assert!(!transparent_cube.is_opaque);
    assert!(!transparent_cube.has_full_face(Direction::West));
    assert!(culler.should_render_face(
        &stone,
        Some(&transparent_cube),
        Direction::East,
        None,
        None,
        None
    ));
}

#[test]
fn test_height_aware_fluid_face_culling() {
    let culler = FaceCuller::default();
    let water = culler.get_meta("minecraft:water[level=0]", None, None);
    let stone = culler.get_meta("minecraft:stone", None, None);
    let bottom_slab = culler.get_meta("minecraft:oak_slab[type=bottom]", None, None);

    // A full block above a non-full fluid surface does not occlude the top face.
    assert!(culler.should_render_fluid_face(&water, None, Some(&stone), Direction::Up, 8.0 / 9.0));
    // Only a fluid surface that actually reaches y = 1.0 is occluded by a ceiling.
    assert!(!culler.should_render_fluid_face(&water, None, Some(&stone), Direction::Up, 1.0));
    // Empty occlusion shapes (glass, leaves, air-like decorations) never occlude.
    let glass = culler.get_meta("minecraft:glass", None, None);
    assert!(culler.should_render_fluid_face(&water, None, Some(&glass), Direction::Up, 1.0));

    // Horizontal faces use the maximum corner height of the side.
    assert!(!culler.should_render_fluid_face(&water, None, Some(&stone), Direction::East, 0.5));
    // A bottom slab only covers the lower half of a fluid side face.
    assert!(culler.should_render_fluid_face(
        &water,
        None,
        Some(&bottom_slab),
        Direction::East,
        8.0 / 9.0
    ));
    // A low flowing surface is fully hidden behind the same slab.
    assert!(!culler.should_render_fluid_face(
        &water,
        None,
        Some(&bottom_slab),
        Direction::East,
        0.4
    ));

    // Same-fluid boundaries and full-block floors stay culled.
    assert!(!culler.should_render_fluid_face(
        &water,
        None,
        Some(&water),
        Direction::East,
        8.0 / 9.0
    ));
    assert!(!culler.should_render_fluid_face(
        &water,
        None,
        Some(&stone),
        Direction::Down,
        8.0 / 9.0
    ));
}
