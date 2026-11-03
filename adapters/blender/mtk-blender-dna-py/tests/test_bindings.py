import array
import ctypes
import unittest

import libmtk_blender_dna as dna


class TestBlenderDna(unittest.TestCase):
    def test_api_version(self):
        self.assertEqual(dna.api_version(), 2)

    def test_copy_f32_typed_and_raw(self):
        src = array.array("f", [1.0, 2.0, 3.0, 4.0])
        dst = (ctypes.c_float * 4)()
        self.assertEqual(
            dna.copy_f32(src, ctypes.addressof(dst), 4, max_bytes=16), 16
        )
        self.assertEqual(list(dst), [1.0, 2.0, 3.0, 4.0])

        dst_raw = (ctypes.c_float * 4)()
        self.assertEqual(
            dna.copy_f32(memoryview(src).cast("B"), ctypes.addressof(dst_raw), 4),
            16,
        )
        self.assertEqual(list(dst_raw), [1.0, 2.0, 3.0, 4.0])

    def test_copy_integer_types(self):
        i32 = array.array("i", [-1, 0, 7])
        dst_i32 = (ctypes.c_int32 * 3)()
        self.assertEqual(dna.copy_i32(i32, ctypes.addressof(dst_i32), 3), 12)
        self.assertEqual(list(dst_i32), [-1, 0, 7])

        u32 = array.array("I", [0, 1, 0xFFFFFFFF])
        dst_u32 = (ctypes.c_uint32 * 3)()
        self.assertEqual(dna.copy_u32(u32, ctypes.addressof(dst_u32), 3), 12)
        self.assertEqual(list(dst_u32), [0, 1, 0xFFFFFFFF])

    def test_copy_u8(self):
        src = bytes([1, 2, 255])
        dst = (ctypes.c_uint8 * 3)()
        self.assertEqual(dna.copy_u8(src, ctypes.addressof(dst), 3), 3)
        self.assertEqual(list(dst), [1, 2, 255])

    def test_rejects_bad_length_and_capacity(self):
        src = array.array("f", [1.0, 2.0])
        dst = (ctypes.c_float * 2)()
        with self.assertRaises(ValueError):
            dna.copy_f32(src, ctypes.addressof(dst), 3)
        with self.assertRaises(ValueError):
            dna.copy_f32(src, ctypes.addressof(dst), 2, max_bytes=4)

    def test_rejects_null_and_misaligned_destination(self):
        src = array.array("f", [1.0, 2.0])
        with self.assertRaises(ValueError):
            dna.copy_f32(src, 0, 2)
        dst = (ctypes.c_float * 2)()
        with self.assertRaises(ValueError):
            dna.copy_f32(src, ctypes.addressof(dst) + 1, 2)

    def test_zero_count_is_noop(self):
        self.assertEqual(dna.copy_f32(b"", 0, 0), 0)
        self.assertEqual(dna.copy_u8(b"", 0, 0), 0)
        dst = bytearray()
        self.assertEqual(dna.read_f32(0, dst, 0), 0)
        self.assertEqual(dna.read_u8(0, dst, 0), 0)
        self.assertEqual(len(dna.as_memoryview_f32(0, 0)), 0)
        self.assertEqual(len(dna.as_memoryview_raw(0, 0)), 0)

    def test_read_f32_typed_and_raw(self):
        src = (ctypes.c_float * 4)(1.5, 2.5, 3.5, 4.5)
        dst = array.array("f", [0.0] * 4)
        self.assertEqual(
            dna.read_f32(ctypes.addressof(src), dst, 4, max_bytes=16), 16
        )
        self.assertEqual(list(dst), [1.5, 2.5, 3.5, 4.5])

        dst_buf = bytearray(16)
        self.assertEqual(
            dna.read_f32(ctypes.addressof(src), dst_buf, 4), 16
        )
        self.assertEqual(list(array.array("f", dst_buf)), [1.5, 2.5, 3.5, 4.5])

    def test_read_integer_types(self):
        src_i32 = (ctypes.c_int32 * 3)(-10, 0, 42)
        dst_i32 = array.array("i", [0] * 3)
        self.assertEqual(dna.read_i32(ctypes.addressof(src_i32), dst_i32, 3), 12)
        self.assertEqual(list(dst_i32), [-10, 0, 42])

        src_u32 = (ctypes.c_uint32 * 3)(0, 100, 0xFFFFFFFF)
        dst_u32 = array.array("I", [0] * 3)
        self.assertEqual(dna.read_u32(ctypes.addressof(src_u32), dst_u32, 3), 12)
        self.assertEqual(list(dst_u32), [0, 100, 0xFFFFFFFF])

        src_u8 = (ctypes.c_uint8 * 3)(1, 128, 255)
        dst_u8 = bytearray(3)
        self.assertEqual(dna.read_u8(ctypes.addressof(src_u8), dst_u8, 3), 3)
        self.assertEqual(list(dst_u8), [1, 128, 255])

    def test_as_memoryview_zerocopy(self):
        src_f32 = (ctypes.c_float * 4)(1.0, 2.0, 3.0, 4.0)
        mv_f32 = dna.as_memoryview_f32(ctypes.addressof(src_f32), 4)
        self.assertEqual(mv_f32.format, "f")
        self.assertEqual(len(mv_f32), 4)
        self.assertEqual(list(mv_f32), [1.0, 2.0, 3.0, 4.0])

        src_i32 = (ctypes.c_int32 * 3)(-5, 10, 25)
        mv_i32 = dna.as_memoryview_i32(ctypes.addressof(src_i32), 3)
        self.assertEqual(mv_i32.format, "i")
        self.assertEqual(list(mv_i32), [-5, 10, 25])

        src_u32 = (ctypes.c_uint32 * 2)(123, 456)
        mv_u32 = dna.as_memoryview_u32(ctypes.addressof(src_u32), 2)
        self.assertEqual(mv_u32.format, "I")
        self.assertEqual(list(mv_u32), [123, 456])

        src_raw = (ctypes.c_uint8 * 4)(10, 20, 30, 40)
        mv_raw = dna.as_memoryview_raw(ctypes.addressof(src_raw), 4)
        self.assertEqual(mv_raw.format, "B")
        self.assertEqual(list(mv_raw), [10, 20, 30, 40])


if __name__ == "__main__":
    unittest.main()
