import sys
import os
import json
import math
from pathlib import Path

# Resolve the MoziToolKit workspace relative to this repo (override with MTK_MOZI_DIR).
WORKSPACE_ROOT = Path(__file__).resolve().parents[2]
MOZI_DIR = Path(os.environ.get("MTK_MOZI_DIR", WORKSPACE_ROOT.parent / "MoziToolKit"))
if str(MOZI_DIR) not in sys.path:
    sys.path.insert(0, str(MOZI_DIR))

from tests._bootstrap import bootstrap_environment
bootstrap_environment()

import zlib
from utils.live_sync.storage import VoxelStorage
from utils.live_sync.classifier import parse_and_classify
from utils.live_sync.meshing.fluid import (
    get_fluid_base_height,
    sample_fluid_height,
    calculate_corner_average,
    calculate_fluid_corner_heights,
    calculate_fluid_flow_vector,
    is_fluid_flowing,
    is_fluid_block,
    MAX_FLUID_HEIGHT,
)
from utils.mesh.fluid_uv import get_fluid_top_uvs, get_fluid_side_uvs
from utils.materials.biome.biome import get_biome_colors, get_colormap_uv
from utils.live_sync.meshing.geometry import RawSectionGeometryBuffer, generate_section_geometry_buffer
from utils.live_sync.meshing.cache import get_cached_state_meta
from utils.live_sync.material import LiveSyncMaterialManager
from utils.mc_baker import get_shared_state_baker

def export_truth_data():
    truth = {
        "fluid_tests": [],
        "biome_tests": [],
        "fluid_uv_tests": [],
        "mesh_tests": [],
    }

    # 1. Fluid Height & Vector Tests
    fluid_scenes = [
        {
            "name": "single_source_water",
            "fluid_type": "water",
            "pos": [0, 0, 0],
            "own_state": "minecraft:water[level=0]",
            "block_map": {(0, 0, 0): "minecraft:water[level=0]"},
        },
        {
            "name": "water_in_solid_stone_corner_jmc2obj",
            "fluid_type": "water",
            "pos": [1, 0, 1],
            "own_state": "minecraft:water[level=0]",
            "block_map": {
                (1, 0, 1): "minecraft:water[level=0]",
                (0, 0, 1): "minecraft:stone",
                (1, 0, 0): "minecraft:stone",
                (0, 0, 0): "minecraft:stone",
            },
        },
        {
            "name": "water_flowing_east_slope",
            "fluid_type": "water",
            "pos": [0, 0, 0],
            "own_state": "minecraft:water[level=0]",
            "block_map": {
                (0, 0, 0): "minecraft:water[level=0]",
                (1, 0, 0): "minecraft:water[level=2]",
            },
        },
        {
            "name": "flowing_water_downstream_block",
            "fluid_type": "water",
            "pos": [1, 0, 0],
            "own_state": "minecraft:water[level=2]",
            "block_map": {
                (0, 0, 0): "minecraft:water[level=0]",
                (1, 0, 0): "minecraft:water[level=2]",
                (2, 0, 0): "minecraft:water[level=4]",
            },
        },
        {
            "name": "submerged_waterfall_block",
            "fluid_type": "water",
            "pos": [0, 0, 0],
            "own_state": "minecraft:water[level=0]",
            "block_map": {
                (0, 0, 0): "minecraft:water[level=0]",
                (0, 1, 0): "minecraft:water[level=0]",
            },
        },
        {
            "name": "cross_fountain_south_slope",
            "fluid_type": "water",
            "pos": [0, 0, 1],
            "own_state": "minecraft:water[level=1]",
            "block_map": {
                (0, 0, 0): "minecraft:oak_slab[type=bottom,waterlogged=true]",
                (0, 0, 1): "minecraft:water[level=1]",
                (1, 0, 0): "minecraft:water[level=1]",
                (0, 0, -1): "minecraft:water[level=1]",
                (-1, 0, 0): "minecraft:water[level=1]",
            },
        },
        {
            "name": "cross_fountain_east_slope",
            "fluid_type": "water",
            "pos": [1, 0, 0],
            "own_state": "minecraft:water[level=1]",
            "block_map": {
                (0, 0, 0): "minecraft:oak_slab[type=bottom,waterlogged=true]",
                (0, 0, 1): "minecraft:water[level=1]",
                (1, 0, 0): "minecraft:water[level=1]",
                (0, 0, -1): "minecraft:water[level=1]",
                (-1, 0, 0): "minecraft:water[level=1]",
            },
        },
        {
            "name": "cross_fountain_north_slope",
            "fluid_type": "water",
            "pos": [0, 0, -1],
            "own_state": "minecraft:water[level=1]",
            "block_map": {
                (0, 0, 0): "minecraft:oak_slab[type=bottom,waterlogged=true]",
                (0, 0, 1): "minecraft:water[level=1]",
                (1, 0, 0): "minecraft:water[level=1]",
                (0, 0, -1): "minecraft:water[level=1]",
                (-1, 0, 0): "minecraft:water[level=1]",
            },
        },
        {
            "name": "cross_fountain_west_slope",
            "fluid_type": "water",
            "pos": [-1, 0, 0],
            "own_state": "minecraft:water[level=1]",
            "block_map": {
                (0, 0, 0): "minecraft:oak_slab[type=bottom,waterlogged=true]",
                (0, 0, 1): "minecraft:water[level=1]",
                (1, 0, 0): "minecraft:water[level=1]",
                (0, 0, -1): "minecraft:water[level=1]",
                (-1, 0, 0): "minecraft:water[level=1]",
            },
        },
    ]

    for sc in fluid_scenes:
        bmap = sc["block_map"]
        x, y, z = sc["pos"]
        ft = sc["fluid_type"]
        state_str = sc["own_state"]
        own_h = get_fluid_base_height(state_str)
        c_nw, c_ne, c_se, c_sw = calculate_fluid_corner_heights(bmap, x, y, z, ft)
        vx, vz, angle = calculate_fluid_flow_vector(bmap, x, y, z, ft, own_h)
        is_flow = is_fluid_flowing(state_str, bmap, x, y, z, ft, vx, vz)

        str_bmap = {f"{k[0]},{k[1]},{k[2]}": v for k, v in bmap.items()}

        truth["fluid_tests"].append({
            "name": sc["name"],
            "fluid_type": ft,
            "pos": [x, y, z],
            "own_state": state_str,
            "block_map": str_bmap,
            "own_height": own_h,
            "corner_heights": [c_nw, c_ne, c_se, c_sw],
            "flow_vector": [vx, vz],
            "flow_angle": angle,
            "is_flowing": is_flow,
        })

    # 3. Biome Colormap UV Tests
    biomes = ["minecraft:plains", "minecraft:desert", "minecraft:ocean", "minecraft:forest", "minecraft:swamp", "minecraft:jungle", "minecraft:taiga"]
    for b in biomes:
        bc = get_biome_colors(b)
        temp = float(bc.get("temperature", 0.8))
        hum = float(bc.get("humidity", 0.4))
        u, v = get_colormap_uv(temp, hum)
        water_col = bc.get("water_linear", (0.05, 0.17, 0.77, 0.8))
        truth["biome_tests"].append({
            "biome": b,
            "temperature": temp,
            "humidity": hum,
            "colormap_uv": [u, v],
            "water_color_linear": list(water_col),
        })

    # 4. Fluid UV Tests
    truth["fluid_uv_tests"].append({
        "name": "still_top",
        "is_flowing": False,
        "rotation": 0.0,
        "expected_uvs": [list(uv) for uv in get_fluid_top_uvs(is_flowing=False, rotation=0.0)],
    })
    for angle in [0.0, math.pi / 4.0, math.pi / 2.0, -math.pi / 2.0, math.pi, -math.pi]:
        truth["fluid_uv_tests"].append({
            "name": f"flowing_top_rot_{round(angle, 4)}",
            "is_flowing": True,
            "rotation": angle,
            "expected_uvs": [list(uv) for uv in get_fluid_top_uvs(is_flowing=True, rotation=angle)],
        })
    truth["fluid_uv_tests"].append({
        "name": "side_uvs_slanted_1.0_0.5",
        "h_left": 1.0,
        "h_right": 0.5,
        "expected_side_uvs": [list(uv) for uv in get_fluid_side_uvs(1.0, 0.5)],
    })

    # 5. Full Section Geometry Buffer Tests
    mock_mapping = {
        "categories": {
            "blocks": {"chunk_id": 0, "tile_size": 16, "width": 512, "height": 512},
            "animations": {"chunk_id": 1, "kind": "animation", "tile_size": 16, "width": 512, "height": 512},
        },
        "textures": {
            "minecraft:block/stone": {"chunk_id": 0, "category": "blocks", "tile_column": 0, "tile_row": 0},
            "minecraft:block/water_still": {"chunk_id": 1, "kind": "animation", "pixel_x": 0, "pixel_y": 0, "frame_width": 16, "frame_height": 16, "frame_count": 32, "frametime": 2},
            "minecraft:block/water_flow": {"chunk_id": 1, "kind": "animation", "pixel_x": 16, "pixel_y": 0, "frame_width": 32, "frame_height": 32, "frame_count": 32, "frametime": 2},
        },
    }
    atlas_params = {"mapping": mock_mapping, "pack_hash": "test_hash"}
    mat_mgr = LiveSyncMaterialManager(world_obj=None, atlas_params=atlas_params)
    baker = get_shared_state_baker()

    mesh_test_scenes = [
        {
            "name": "single_cube_mesh",
            "blocks": {(0, 0, 0): "minecraft:stone"},
        },
        {
            "name": "two_touching_cubes_mesh",
            "blocks": {(0, 0, 0): "minecraft:stone", (1, 0, 0): "minecraft:stone"},
        },
        {
            "name": "water_on_stone_mesh",
            "blocks": {(0, 0, 0): "minecraft:stone", (0, 1, 0): "minecraft:water[level=0]"},
        },
    ]

    for msc in mesh_test_scenes:
        storage = VoxelStorage()
        storage.set_bounds(0, 0, 0, 16, 16, 16)
        for (bx, by, bz), st in msc["blocks"].items():
            storage.set_block(bx, by, bz, st)

        state_cache = {s: get_cached_state_meta(s, mat_mgr, baker) for s in set(msc["blocks"].values())}

        buf = generate_section_geometry_buffer(
            voxel_items=list(msc["blocks"].items()),
            block_map=storage.block_map,
            state_cache=state_cache,
            origin_centered=False,
            min_x=0, min_y=0, min_z=0,
            half_x=0.0, half_z=0.0,
            mat_manager=mat_mgr,
            baker=baker,
            voxel_storage=storage,
            weld_vertices=False,
        )

        str_blocks = {f"{k[0]},{k[1]},{k[2]}": v for k, v in msc["blocks"].items()}
        truth["mesh_tests"].append({
            "name": msc["name"],
            "blocks": str_blocks,
            "face_count": len(buf.faces),
            "vertex_count": len(buf.vertices),
            "vertices": [list(v) for v in buf.vertices],
            "cubes_count": buf.cubes_count,
            "fluids_count": buf.fluids_count,
        })

    out_path = Path(
        os.environ.get(
            "MTK_GROUND_TRUTH_OUT",
            WORKSPACE_ROOT / "crates/mtk-voxel/tests/fixtures/blender_ground_truth.json",
        )
    )
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(truth, f, indent=2)

    print(f"Exported ground truth to {out_path} ({len(truth['fluid_tests'])} fluid tests, {len(truth['mesh_tests'])} mesh tests)")

if __name__ == "__main__":
    export_truth_data()
