import json
import pytest
import libmtk_py


SAMPLE_ATLAS_MAPPING = json.dumps({
    "format_version": 2,
    "chunk_size": 1024,
    "chunks": [
        {
            "chunk_id": 0,
            "category": "entities",
            "category_chunk_index": 0,
            "width": 1024,
            "height": 1024,
            "has_normal": False,
            "has_specular": False,
            "has_orm": False,
        }
    ],
    "sprites": {
        "minecraft:painting/kebab": {
            "chunk_id": 0,
            "texture_id": 1,
            "uv_bounds": [0.0, 0.0, 0.5, 0.5],
            "is_animated": False,
            "frame_count": 1,
            "has_normal": False,
            "has_specular": False,
        },
        "minecraft:painting/back": {
            "chunk_id": 0,
            "texture_id": 2,
            "uv_bounds": [0.5, 0.0, 1.0, 0.5],
            "is_animated": False,
            "frame_count": 1,
            "has_normal": False,
            "has_specular": False,
        },
    },
})


def test_point_cloud_visual_entities_palette():
    cloud = libmtk_py.VoxelPointCloud()
    assert not cloud.has_visual_entities()
    assert cloud.visual_entities_count() == 0
    assert cloud.get_visual_entities_palette() == []

    entities = [
        {
            "id": "minecraft:painting",
            "position": [10.5, 64.0, -20.5],
            "facing": "south",
            "rotation": [0.0, 0.0, 0.0],
            "kind": {
                "type": "minecraft:painting",
                "variant": "kebab",
                "width": 1,
                "height": 1,
            },
        }
    ]
    palette = [json.dumps(e) for e in entities]
    cloud.set_visual_entities_palette(palette)
    assert cloud.has_visual_entities()
    assert cloud.visual_entities_count() == 1

    extracted = cloud.get_visual_entities_palette()
    assert len(extracted) == 1
    parsed = json.loads(extracted[0])
    assert parsed["id"] == "minecraft:painting"
    assert parsed["kind"]["variant"] == "kebab"

    # Roundtrip through storage
    storage = cloud.to_storage()
    assert storage.has_visual_entities()
    assert len(storage.get_visual_entities_palette()) == 1

    storage.set_bounds(0, -64, 0, 16, 128, 16)
    assert storage.bounds == [0, -64, 0, 16, 128, 16]

    cloud2 = storage.to_point_cloud()
    assert cloud2.has_visual_entities()
    assert cloud2.visual_entities_count() == 1

    # Lossless roundtrip via to_buffers()
    bufs = cloud2.to_buffers()
    assert "visual_entities" in bufs
    assert len(bufs["visual_entities"]) == 1

    # Reconstruct from indexed arrays with visual_entities
    import numpy as np
    reconstructed = libmtk_py.VoxelPointCloud.from_indexed_arrays(
        np.array([], dtype=np.float32),
        np.array([], dtype=np.int32),
        np.array([], dtype=np.int32),
        np.array([], dtype=np.int32),
        np.array([], dtype=np.int32),
        ["minecraft:air"],
        visual_entities=bufs["visual_entities"],
    )
    assert reconstructed.has_visual_entities()
    assert reconstructed.visual_entities_count() == 1
    assert reconstructed.get_visual_entities_palette() == bufs["visual_entities"]

    # Validation rejection test
    with pytest.raises(ValueError):
        # Invalid scale (zero)
        cloud.set_visual_entities_palette([json.dumps({
            "id": "minecraft:painting",
            "position": [0, 0, 0],
            "rotation": [0, 0, 0],
            "scale": [0, 0, 0],
            "kind": {"type": "minecraft:painting", "variant": "kebab", "width": 1, "height": 1}
        })])


def test_build_entity_render_plan_top_level():
    entities = [
        {
            "id": "minecraft:painting",
            "uuid": "12345678-1234-1234-1234-123456789abc",
            "position": [10.5, 64.0, -20.5],
            "facing": "south",
            "rotation": [0.0, 3.14159265, 0.0],
            "scale": [1.0, 1.0, 1.0],
            "kind": {
                "type": "minecraft:painting",
                "variant": "kebab",
                "width": 1,
                "height": 1,
            },
        }
    ]

    # 1. Build from list of dicts with Z-Up coordinate system and bounds
    plan = libmtk_py.build_entity_render_plan(
        entities,
        atlas_mapping_json=SAMPLE_ATLAS_MAPPING,
        bounds=[0, 0, 0, 16, 16, 16],
        origin_centered=True,
        coordinate_system="z_up",
        revision=42,
        authoritative=True,
    )

    assert plan.schema_version == 1
    assert plan.revision == 42
    assert plan.authoritative is True
    assert plan.bounds == [0, 0, 0, 16, 16, 16]
    assert plan.node_count == 1
    assert len(plan.warnings) == 0

    node = plan.nodes[0]
    assert node.key == "uuid:12345678-1234-1234-1234-123456789abc"
    assert "kebab" in node.display_name.lower() or "painting" in node.display_name.lower()

    # Verify host transform (origin centered at bottom center of bounds [8, 0, 8])
    # x = 10.5 - 8.0 = 2.5
    # y = -(-20.5 - 8.0) = 28.5
    # z = 64.0 - 0.0 = 64.0
    loc = node.location
    assert abs(loc[0] - 2.5) < 1e-4
    assert abs(loc[1] - 28.5) < 1e-4
    assert abs(loc[2] - 64.0) < 1e-4

    rot = node.rotation_euler
    assert abs(abs(rot[2]) - 3.14159265) < 1e-3

    scale = node.scale
    assert scale == (1.0, 1.0, 1.0)

    matrix = node.matrix
    assert len(matrix) == 16

    # Verify baked mesh
    mesh = node.mesh
    assert mesh.vertex_count > 0
    assert mesh.face_count > 0

    # Verify mesh local coordinates in Z-up right-handed system (X: width, Y: thickness, Z: height)
    pos_mv = mesh.positions_memoryview()
    xs = [pos_mv[i * 3] for i in range(mesh.vertex_count)]
    ys = [pos_mv[i * 3 + 1] for i in range(mesh.vertex_count)]
    zs = [pos_mv[i * 3 + 2] for i in range(mesh.vertex_count)]
    assert abs(min(xs) - (-0.5)) < 1e-4 and abs(max(xs) - 0.5) < 1e-4
    assert abs(min(ys) - (-0.03125)) < 1e-4 and abs(max(ys) - 0.03125) < 1e-4
    assert abs(min(zs) - (-0.5)) < 1e-4 and abs(max(zs) - 0.5) < 1e-4

    # Verify routes
    routes = node.material_routes
    assert len(routes) == 2
    assert routes[0]["texture_key"] in ("minecraft:painting/kebab", "minecraft:painting/back")
    assert routes[0]["atlas_chunk_id"] == 0

    # Verify metadata
    meta = node.metadata
    assert meta["variant"] == "kebab"
    assert meta["uuid"] == "12345678-1234-1234-1234-123456789abc"

    # 2. Build with unsupported entity reports warning
    entities_with_unsupported = entities + [
        {
            "id": "minecraft:item_frame",
            "position": [5.0, 65.0, 3.0],
            "rotation": [0.0, 0.0, 0.0],
            "kind": {
                "type": "minecraft:item_frame",
                "item_id": "minecraft:diamond_sword",
            },
        }
    ]
    plan_with_warn = libmtk_py.build_entity_render_plan(
        entities_with_unsupported,
        atlas_mapping_json=SAMPLE_ATLAS_MAPPING,
    )
    assert plan_with_warn.node_count == 1
    assert len(plan_with_warn.warnings) == 1
    assert plan_with_warn.warnings[0]["code"] == "unsupported_entity"


def test_storage_and_cloud_entities_palette_build_entity_render_plan():
    storage = libmtk_py.VoxelStorage()
    storage.set_bounds(0, 0, 0, 16, 16, 16)

    entities = [
        {
            "id": "minecraft:painting",
            "uuid": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "position": [10.5, 64.0, -20.5],
            "facing": "south",
            "rotation": [0.0, 0.0, 0.0],
            "kind": {
                "type": "minecraft:painting",
                "variant": "kebab",
                "width": 1,
                "height": 1,
            },
        }
    ]
    storage.set_visual_entities_palette([json.dumps(e) for e in entities])

    cfg = libmtk_py.MesherConfig(origin_centered=True)
    plan = libmtk_py.build_entity_render_plan(
        storage.get_visual_entities_palette(),
        config=cfg,
        bounds=[0, 0, 0, 16, 16, 16],
        atlas_mapping_json=SAMPLE_ATLAS_MAPPING,
    )
    assert plan.node_count == 1
    assert plan.nodes[0].key == "uuid:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"

    # Convert to cloud and build plan from cloud palette
    cloud = storage.to_point_cloud(cfg)
    plan_cloud = libmtk_py.build_entity_render_plan(
        cloud.get_visual_entities_palette(),
        config=cfg,
        bounds=[0, 0, 0, 16, 16, 16],
        atlas_mapping_json=SAMPLE_ATLAS_MAPPING,
    )
    assert plan_cloud.node_count == 1
    assert plan_cloud.nodes[0].key == "uuid:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"


def test_host_transform_to_mc_python_binding():
    # 1. Roundtrip with origin centered and bounds in Z-Up
    bounds = [0, 0, 0, 16, 16, 16]
    orig_pos = (10.5, 64.0, -20.5)
    orig_rot = (0.2, -0.4, 0.6)
    orig_scale = (1.0, 1.0, 1.0)

    entities = [
        {
            "id": "minecraft:painting",
            "position": list(orig_pos),
            "rotation": list(orig_rot),
            "scale": list(orig_scale),
            "kind": {"type": "minecraft:painting", "variant": "kebab", "width": 1, "height": 1},
        }
    ]
    plan = libmtk_py.build_entity_render_plan(
        entities,
        bounds=bounds,
        origin_centered=True,
        coordinate_system="z_up",
    )
    node = plan.nodes[0]
    loc = node.location
    rot = node.rotation_euler
    scale = node.scale

    back_pos, back_rot, back_scale = libmtk_py.host_transform_to_mc(
        loc,
        rot,
        scale,
        bounds=bounds,
        origin_centered=True,
        coordinate_system="z_up",
    )

    assert abs(back_pos[0] - orig_pos[0]) < 1e-4
    assert abs(back_pos[1] - orig_pos[1]) < 1e-4
    assert abs(back_pos[2] - orig_pos[2]) < 1e-4
    assert abs(back_rot[0] - orig_rot[0]) < 1e-4
    assert abs(back_rot[1] - orig_rot[1]) < 1e-4
    assert abs(back_rot[2] - orig_rot[2]) < 1e-4
    assert abs(back_scale[0] - orig_scale[0]) < 1e-4


def test_mesher_config_coordinate_system_options():
    cfg_z = libmtk_py.MesherConfig(coordinate_system="z_up")
    assert cfg_z.coordinate_system == "z_up"
    assert cfg_z.z_up_coordinates is True

    cfg_y = libmtk_py.MesherConfig(coordinate_system="y_up")
    assert cfg_y.coordinate_system == "y_up"
    assert cfg_y.z_up_coordinates is False

    cfg_mc = libmtk_py.MesherConfig(coordinate_system="minecraft")
    assert cfg_mc.coordinate_system == "minecraft"
    assert cfg_mc.z_up_coordinates is False

    with pytest.raises(ValueError):
        libmtk_py.MesherConfig(coordinate_system="invalid_sys")


def test_build_entity_render_plan_delta_and_deleted_keys():
    entities = [
        {
            "id": "minecraft:painting",
            "uuid": "survivor-1111",
            "position": [10.5, 64.0, -20.5],
            "facing": "south",
            "rotation": [0.0, 3.14159265, 0.0],
            "kind": {"type": "minecraft:painting", "variant": "kebab", "width": 1, "height": 1},
        }
    ]

    # 1. Delta plan with 1 node and 2 deleted keys
    plan = libmtk_py.build_entity_render_plan(
        entities,
        deleted_keys=["uuid:dead-1", "uuid:dead-2"],
        authoritative=False,
        revision=101,
    )
    assert plan.authoritative is False
    assert plan.revision == 101
    assert plan.node_count == 1
    assert plan.deleted_keys == ["uuid:dead-1", "uuid:dead-2"]

    # 2. Deletion-only delta plan with entities=None
    plan_del_none = libmtk_py.build_entity_render_plan(
        None,
        deleted_keys=["uuid:dead-1"],
        authoritative=False,
        revision=102,
    )
    assert plan_del_none.authoritative is False
    assert plan_del_none.node_count == 0
    assert plan_del_none.deleted_keys == ["uuid:dead-1"]

    # 3. Deletion-only delta plan with entities=[]
    plan_del_empty = libmtk_py.build_entity_render_plan(
        [],
        deleted_keys=["uuid:dead-3"],
        authoritative=False,
    )
    assert plan_del_empty.authoritative is False
    assert plan_del_empty.node_count == 0
    assert plan_del_empty.deleted_keys == ["uuid:dead-3"]


