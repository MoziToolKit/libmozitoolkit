//! Lightweight, zero-copy, host-agnostic NBT decoder for Minecraft saves.
//!
//! Provides safe, fast, 100% Stable Rust and WebAssembly (`wasm32-unknown-unknown`)
//! compatible parsing of Minecraft Java Edition NBT data structures.

mod error;
mod reader;
mod tag;

use std::io::Cursor;

pub(crate) use error::NbtError;
pub(crate) use reader::*;
pub(crate) use tag::*;

/// The decoded root NBT structure.
#[derive(Debug, Clone, PartialEq)]
pub enum Nbt<'a> {
    /// A valid root compound tag and its name.
    Some(BaseNbt<'a>),
    /// Empty or end tag.
    None,
}

/// The root compound tag container.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseNbt<'a> {
    /// Root tag name (typically empty string in `level.dat` and MCA chunks).
    pub name: NbtStr<'a>,
    /// Root compound tag.
    pub root: NbtCompound<'a>,
}

impl<'a> BaseNbt<'a> {
    /// Borrows the inner compound tag.
    #[inline]
    pub fn as_compound(&self) -> &NbtCompound<'a> {
        &self.root
    }
}

/// Reads a root NBT compound from a `Cursor<&'a [u8]>`.
///
/// Updates the cursor position to the end of the parsed NBT data.
pub(crate) fn read<'a>(cursor: &mut Cursor<&'a [u8]>) -> Result<Nbt<'a>, NbtError> {
    let pos = cursor.position() as usize;
    let slice = cursor.get_ref();
    if pos >= slice.len() {
        return Ok(Nbt::None);
    }

    let mut reader = SliceReader::new(&slice[pos..]);
    let root_type = reader.read_u8()?;
    if root_type == TAG_END {
        cursor.set_position((pos + reader.pos) as u64);
        return Ok(Nbt::None);
    }

    if root_type != TAG_COMPOUND {
        return Err(NbtError::InvalidRootType(root_type));
    }

    let name = reader.read_str()?;
    let root = reader::read_compound_payload(&mut reader, 0)?;
    cursor.set_position((pos + reader.pos) as u64);

    Ok(Nbt::Some(BaseNbt { name, root }))
}

#[cfg(test)]
/// Reads a root NBT compound directly from an in-memory byte slice.
pub(crate) fn read_from_slice<'a>(slice: &'a [u8]) -> Result<Nbt<'a>, NbtError> {
    let mut cursor = Cursor::new(slice);
    read(&mut cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_nbt() {
        let bytes = [TAG_END];
        let nbt = read_from_slice(&bytes).expect("Should parse empty NBT");
        assert_eq!(nbt, Nbt::None);
    }

    #[test]
    fn test_simple_compound_roundtrip() {
        // Construct a synthetic NBT compound:
        // Root: TAG_Compound (10), name_len = 0, name = ""
        //   TAG_Byte (1), name_len = 1, name = "b", value = 42
        //   TAG_Int (3), name_len = 1, name = "i", value = 1337
        //   TAG_String (8), name_len = 1, name = "s", val_len = 5, val = "hello"
        //   TAG_Long_Array (12), name_len = 1, name = "l", arr_len = 2, vals = [100, 200]
        //   TAG_End (0)
        let mut buf = Vec::new();
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&0u16.to_be_bytes()); // root name ""

        // b = 42
        buf.push(TAG_BYTE);
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(b"b");
        buf.push(42);

        // i = 1337
        buf.push(TAG_INT);
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(b"i");
        buf.extend_from_slice(&1337i32.to_be_bytes());

        // s = "hello"
        buf.push(TAG_STRING);
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(b"s");
        buf.extend_from_slice(&5u16.to_be_bytes());
        buf.extend_from_slice(b"hello");

        // l = [100i64, 200i64]
        buf.push(TAG_LONG_ARRAY);
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(b"l");
        buf.extend_from_slice(&2i32.to_be_bytes());
        buf.extend_from_slice(&100i64.to_be_bytes());
        buf.extend_from_slice(&200i64.to_be_bytes());

        buf.push(TAG_END);

        let nbt = read_from_slice(&buf).expect("Should parse valid synthetic NBT");
        let base = match nbt {
            Nbt::Some(b) => b,
            Nbt::None => panic!("Expected Some NBT"),
        };

        let root = base.as_compound();
        assert_eq!(root.byte("b"), Some(42));
        assert_eq!(root.int("i"), Some(1337));
        assert_eq!(root.string("s").as_deref(), Some("hello"));
        assert_eq!(root.long_array("l"), Some(vec![100, 200]));
        assert_eq!(root.byte("non_existent"), None);
    }
}
