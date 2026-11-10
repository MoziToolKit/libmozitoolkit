import unittest

import libmtk_py


def quad_mesh():
    return libmtk_py.MeshData.from_flat_polygons(
        [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        [0, 1, 2, 3],
        [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
        [0],
        [4],
        None,
        [0],
    )


class TestLibMtkPy(unittest.TestCase):
    def test_version(self):
        self.assertEqual(libmtk_py.api_version(), 2)

    def test_mesh_data_buffers(self):
        mesh = quad_mesh()
        self.assertEqual(mesh.vertex_count, 4)
        self.assertEqual(mesh.triangle_count, 2)
        self.assertEqual(mesh.face_count, 1)
        self.assertEqual(mesh.quad_count, 1)

        pos_mv = mesh.positions_memoryview()
        self.assertEqual(len(pos_mv), 4 * 3)
        self.assertEqual(pos_mv.format, "f")
        self.assertEqual(pos_mv.nbytes, 4 * 3 * 4)

        uv_mv = mesh.uvs_memoryview()
        self.assertEqual(len(uv_mv), 4 * 2)
        self.assertEqual(uv_mv.format, "f")
        self.assertEqual(uv_mv.nbytes, 4 * 2 * 4)

        idx_mv = mesh.indices_memoryview()
        self.assertEqual(len(idx_mv), 6)
        self.assertEqual(idx_mv.format, "I")
        self.assertEqual(idx_mv.nbytes, 6 * 4)

        quad_mv = mesh.quad_indices_memoryview()
        self.assertEqual(len(quad_mv), 4)
        self.assertEqual(quad_mv.format, "I")
        self.assertEqual(quad_mv.nbytes, 4 * 4)

    def test_custom_attributes_memoryview_and_list(self):
        import numpy as np

        mesh = quad_mesh()
        mesh.add_string_attribute("mtk_source_texture", "face", ["minecraft:block/dirt"])
        mesh.add_attribute_from_buffer(
            "mtk_emission", "face", "float", bytes([0, 0, 128, 63])
        )

        self.assertIn("mtk_source_texture", mesh.attribute_names())
        self.assertIn("mtk_emission", mesh.attribute_names())
        self.assertEqual(
            mesh.get_string_attribute("mtk_source_texture"),
            ["minecraft:block/dirt"],
        )
        emission_mv = mesh.attribute_memoryview("mtk_emission")
        self.assertIsNotNone(emission_mv)
        self.assertAlmostEqual(np.frombuffer(emission_mv, dtype=np.float32)[0], 1.0)

    def test_mesh_data_memoryviews(self):
        import numpy as np

        mesh = quad_mesh()
        pos = np.frombuffer(mesh.positions_memoryview(), dtype=np.float32)
        self.assertEqual(len(pos), 12)
        np.testing.assert_allclose(
            pos.reshape(-1, 3)[1], [1.0, 0.0, 0.0]
        )

        idx = np.frombuffer(mesh.indices_memoryview(), dtype=np.uint32)
        np.testing.assert_array_equal(idx, [0, 1, 2, 0, 2, 3])

        loop_uv = np.frombuffer(mesh.loop_uvs_memoryview(), dtype=np.float32)
        np.testing.assert_array_equal(
            loop_uv, [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]
        )
        bufs = mesh.to_buffers()
        self.assertEqual(len(bufs["loop_starts"]), 1)
        self.assertEqual(len(bufs["loop_totals"]), 1)

    def test_build_material_palette(self):
        import numpy as np

        palette, indices = libmtk_py.build_material_palette(
            ["Grass", "default"],
            [False, True],
            ["", "minecraft:block/grass_block_top"],
            np.array([1, 0, 1], dtype=np.int32),
        )
        self.assertEqual(
            palette, ["minecraft:block/grass_block_top", "Grass"]
        )
        np.testing.assert_array_equal(
            np.frombuffer(indices, dtype=np.uint32), [0, 1, 0]
        )

    def test_point_cloud_palette_indices(self):
        import numpy as np

        cloud = libmtk_py.VoxelPointCloud()
        cloud.push([0.5, 0.5, 0.5], [0, 0, 0], "minecraft:stone", "minecraft:plains", 0)
        cloud.push([1.5, 0.5, 0.5], [1, 0, 0], "minecraft:dirt", "minecraft:desert", 0)
        cloud.push([2.5, 0.5, 0.5], [2, 0, 0], "minecraft:stone", "minecraft:plains", 0)

        self.assertEqual(
            cloud.get_state_palette(), ["minecraft:stone", "minecraft:dirt"]
        )
        self.assertEqual(
            cloud.get_biome_palette(), ["minecraft:plains", "minecraft:desert"]
        )
        np.testing.assert_array_equal(
            np.frombuffer(cloud.state_indices_memoryview(), dtype=np.int32),
            [0, 1, 0],
        )
        np.testing.assert_array_equal(
            np.frombuffer(cloud.biome_indices_memoryview(), dtype=np.int32),
            [0, 1, 0],
        )

        self.assertFalse(cloud.has_entities())
        self.assertEqual(cloud.get_entity_palette(), [])

    def test_point_cloud_entity_palette_and_from_indexed_arrays(self):
        import numpy as np

        positions = np.array([0.5, 0.5, 0.5, 1.5, 0.5, 0.5], dtype=np.float32)
        bx = np.array([0, 1], dtype=np.int32)
        by = np.array([0, 0], dtype=np.int32)
        bz = np.array([0, 0], dtype=np.int32)
        st_idx = np.array([0, 1], dtype=np.int32)
        st_pal = ["minecraft:stone", "minecraft:decorated_pot"]
        ent_idx = np.array([-1, 0], dtype=np.int32)
        ent_pal = ['{"id":"minecraft:decorated_pot","sherds":["angler",null,null,null]}']

        cloud = libmtk_py.VoxelPointCloud.from_indexed_arrays(
            positions,
            bx,
            by,
            bz,
            st_idx,
            st_pal,
            entity_indices=ent_idx,
            entity_palette=ent_pal,
        )

        self.assertTrue(cloud.has_entities())
        self.assertEqual(cloud.get_entity_palette(), ent_pal)
        np.testing.assert_array_equal(
            np.frombuffer(cloud.entity_indices_memoryview(), dtype=np.int32),
            [-1, 0],
        )

    def test_mesh_data_from_polygon_offsets(self):
        positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0]
        loop_verts = [0, 1, 2, 3]
        loop_uvs = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]
        face_offsets = [0, 4]

        # 1. via from_polygon_offsets
        mesh1 = libmtk_py.MeshData.from_polygon_offsets(
            positions,
            loop_verts,
            loop_uvs,
            face_offsets,
        )
        self.assertEqual(mesh1.vertex_count, 4)
        self.assertEqual(mesh1.face_count, 1)
        self.assertEqual(mesh1.quad_count, 1)

        # 2. via from_flat_polygons with face_offsets keyword
        mesh2 = libmtk_py.MeshData.from_flat_polygons(
            positions=positions,
            loop_vertices=loop_verts,
            loop_uvs=loop_uvs,
            face_offsets=face_offsets,
        )
        self.assertEqual(mesh2.vertex_count, 4)
        self.assertEqual(mesh2.face_count, 1)
        self.assertEqual(mesh2.quad_count, 1)

    def test_repair_mesh_fluid_uv_inplace_overlapping_memory(self):
        import numpy as np

        # Create a single contiguous float32 buffer
        shared_buf = np.zeros(32, dtype=np.float32)
        # Slices with distinct Python objects but overlapping memory spans:
        pos_view = shared_buf[0:12]  # 12 floats = 4 verts * 3
        uvs_view = shared_buf[8:16]  # 8 floats = 4 loops * 2 (overlaps pos at indices 8..11)
        self.assertIsNot(pos_view, uvs_view)

        loops = np.array([0, 1, 2, 3], dtype=np.uint32)
        starts = np.array([0], dtype=np.uint32)
        totals = np.array([4], dtype=np.uint32)

        with self.assertRaises(ValueError) as ctx:
            libmtk_py.repair_mesh_fluid_uv_inplace(
                pos_view,
                loops,
                starts,
                totals,
                uvs_view,
            )
        self.assertIn("must not overlap in memory", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
