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
        self.assertTrue(libmtk_py.version())
        self.assertEqual(libmtk_py.api_version(), 1)

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
        mesh = quad_mesh()
        mesh.add_string_attribute("mtk_source_texture", "face", ["minecraft:block/dirt"])
        mesh.add_attribute_from_buffer(
            "mtk_emission", "face", "float", bytes([0, 0, 128, 63])
        )

        self.assertTrue(mesh.has_attribute("mtk_source_texture"))
        self.assertTrue(mesh.has_attribute("mtk_emission"))
        self.assertEqual(
            mesh.get_string_attribute("mtk_source_texture"),
            ["minecraft:block/dirt"],
        )
        self.assertAlmostEqual(mesh.get_attribute_data("mtk_emission")[0], 1.0)

    def test_add_string_attribute_from_palette(self):
        import numpy as np

        mesh = quad_mesh()
        palette = ["minecraft:block/stone", "minecraft:block/dirt"]
        indices = np.array([1], dtype=np.uint32)
        mesh.add_string_attribute_from_palette(
            "mtk_source_texture_key", "face", palette, indices
        )
        self.assertEqual(
            mesh.get_string_attribute("mtk_source_texture_key"),
            ["minecraft:block/dirt"],
        )

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
        self.assertEqual(len(mesh.loop_starts_memoryview()), 4)
        self.assertEqual(len(mesh.loop_totals_memoryview()), 4)

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

        restored = libmtk_py.VoxelPointCloud.from_mesh_data(cloud.to_mesh_data())
        self.assertEqual(restored.get_state_palette(), cloud.get_state_palette())
        self.assertEqual(restored.get_biome_palette(), cloud.get_biome_palette())
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

        mesh = cloud.to_mesh_data()
        restored = libmtk_py.VoxelPointCloud.from_mesh_data(mesh)
        self.assertTrue(restored.has_entities())
        self.assertEqual(restored.get_entity_palette(), ent_pal)
        np.testing.assert_array_equal(
            np.frombuffer(restored.entity_indices_memoryview(), dtype=np.int32),
            [-1, 0],
        )


if __name__ == "__main__":
    unittest.main()
