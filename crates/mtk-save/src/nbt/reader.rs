use std::borrow::Cow;
use std::str;

use crate::nbt::error::NbtError;
use crate::nbt::tag::{NbtCompound, NbtList, NbtStr, NbtTag};

pub const TAG_END: u8 = 0;
pub const TAG_BYTE: u8 = 1;
pub const TAG_SHORT: u8 = 2;
pub const TAG_INT: u8 = 3;
pub const TAG_LONG: u8 = 4;
pub const TAG_FLOAT: u8 = 5;
pub const TAG_DOUBLE: u8 = 6;
pub const TAG_BYTE_ARRAY: u8 = 7;
pub const TAG_STRING: u8 = 8;
pub const TAG_LIST: u8 = 9;
pub const TAG_COMPOUND: u8 = 10;
pub const TAG_INT_ARRAY: u8 = 11;
pub const TAG_LONG_ARRAY: u8 = 12;

const MAX_DEPTH: usize = 512;

/// A fast, zero-copy binary reader over an in-memory byte slice.
pub struct SliceReader<'a> {
    slice: &'a [u8],
    pub pos: usize,
}

impl<'a> SliceReader<'a> {
    #[inline]
    pub fn new(slice: &'a [u8]) -> Self {
        Self { slice, pos: 0 }
    }

    #[inline]
    pub fn read_u8(&mut self) -> Result<u8, NbtError> {
        if self.pos >= self.slice.len() {
            return Err(NbtError::UnexpectedEof);
        }
        let b = self.slice[self.pos];
        self.pos += 1;
        Ok(b)
    }

    #[inline]
    pub fn read_i8(&mut self) -> Result<i8, NbtError> {
        self.read_u8().map(|b| b as i8)
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.slice.len().saturating_sub(self.pos)
    }

    #[inline]
    pub fn read_exact(&mut self, n: usize) -> Result<&'a [u8], NbtError> {
        let new_pos = self.pos.checked_add(n).ok_or(NbtError::UnexpectedEof)?;
        if new_pos > self.slice.len() {
            return Err(NbtError::UnexpectedEof);
        }
        let sub = &self.slice[self.pos..new_pos];
        self.pos = new_pos;
        Ok(sub)
    }

    #[inline]
    pub fn read_u16_be(&mut self) -> Result<u16, NbtError> {
        let b = self.read_exact(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    #[inline]
    pub fn read_i16_be(&mut self) -> Result<i16, NbtError> {
        let b = self.read_exact(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    #[inline]
    pub fn read_i32_be(&mut self) -> Result<i32, NbtError> {
        let b = self.read_exact(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[inline]
    pub fn read_i64_be(&mut self) -> Result<i64, NbtError> {
        let b = self.read_exact(8)?;
        Ok(i64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    #[inline]
    pub fn read_f32_be(&mut self) -> Result<f32, NbtError> {
        let b = self.read_exact(4)?;
        Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[inline]
    pub fn read_f64_be(&mut self) -> Result<f64, NbtError> {
        let b = self.read_exact(8)?;
        Ok(f64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    #[inline]
    pub fn read_str(&mut self) -> Result<NbtStr<'a>, NbtError> {
        let len = self.read_u16_be()? as usize;
        if self.remaining() < len {
            return Err(NbtError::InvalidData(format!(
                "String length {len} exceeds remaining stream size {}",
                self.remaining()
            )));
        }
        let bytes = self.read_exact(len)?;
        let s = match str::from_utf8(bytes) {
            Ok(v) => Cow::Borrowed(v),
            Err(_) => String::from_utf8_lossy(bytes),
        };
        Ok(NbtStr(s))
    }
}

pub fn read_compound_payload<'a>(
    reader: &mut SliceReader<'a>,
    depth: usize,
) -> Result<NbtCompound<'a>, NbtError> {
    if depth > MAX_DEPTH {
        return Err(NbtError::InvalidData(
            "Exceeded maximum NBT compound recursion depth".into(),
        ));
    }

    let mut entries = Vec::new();
    loop {
        let tag_id = reader.read_u8()?;
        if tag_id == TAG_END {
            break;
        }
        let name = reader.read_str()?;
        let value = read_tag_payload(reader, tag_id, depth + 1)?;
        entries.push((name, value));
    }
    Ok(NbtCompound { entries })
}

pub fn read_tag_payload<'a>(
    reader: &mut SliceReader<'a>,
    tag_id: u8,
    depth: usize,
) -> Result<NbtTag<'a>, NbtError> {
    if depth > MAX_DEPTH {
        return Err(NbtError::InvalidData(
            "Exceeded maximum NBT recursion depth".into(),
        ));
    }

    match tag_id {
        TAG_BYTE => Ok(NbtTag::Byte(reader.read_i8()?)),
        TAG_SHORT => Ok(NbtTag::Short(reader.read_i16_be()?)),
        TAG_INT => Ok(NbtTag::Int(reader.read_i32_be()?)),
        TAG_LONG => Ok(NbtTag::Long(reader.read_i64_be()?)),
        TAG_FLOAT => Ok(NbtTag::Float(reader.read_f32_be()?)),
        TAG_DOUBLE => Ok(NbtTag::Double(reader.read_f64_be()?)),
        TAG_BYTE_ARRAY => {
            let len = reader.read_i32_be()?;
            if len < 0 {
                return Err(NbtError::InvalidData(format!(
                    "Negative byte array length: {len}"
                )));
            }
            let count = len as usize;
            if reader.remaining() < count {
                return Err(NbtError::InvalidData(format!(
                    "Byte array length {count} exceeds remaining stream size {}",
                    reader.remaining()
                )));
            }
            let bytes = reader.read_exact(count)?;
            Ok(NbtTag::ByteArray(bytes))
        }
        TAG_STRING => Ok(NbtTag::String(reader.read_str()?)),
        TAG_LIST => {
            let item_type = reader.read_u8()?;
            let len = reader.read_i32_be()?;
            if len < 0 {
                return Err(NbtError::InvalidData(format!(
                    "Negative list length: {len}"
                )));
            }
            if len == 0 {
                return Ok(NbtTag::List(NbtList::Empty));
            }
            let count = len as usize;
            let min_size = match item_type {
                TAG_BYTE => 1,
                TAG_SHORT => 2,
                TAG_INT => 4,
                TAG_LONG => 8,
                TAG_FLOAT => 4,
                TAG_DOUBLE => 8,
                TAG_BYTE_ARRAY => 4,
                TAG_STRING => 2,
                TAG_LIST => 5,
                TAG_COMPOUND => 1,
                TAG_INT_ARRAY => 4,
                TAG_LONG_ARRAY => 4,
                unknown => return Err(NbtError::UnknownTagId(unknown)),
            };
            let required_bytes = count.checked_mul(min_size).ok_or_else(|| {
                NbtError::InvalidData(format!(
                    "List length {count} overflows size calculation for element type {item_type}"
                ))
            })?;
            if reader.remaining() < required_bytes {
                return Err(NbtError::InvalidData(format!(
                    "List length {count} of type {item_type} requires at least {required_bytes} bytes, but only {} bytes remain",
                    reader.remaining()
                )));
            }
            match item_type {
                TAG_BYTE => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_i8()?);
                    }
                    Ok(NbtTag::List(NbtList::Byte(vec)))
                }
                TAG_SHORT => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_i16_be()?);
                    }
                    Ok(NbtTag::List(NbtList::Short(vec)))
                }
                TAG_INT => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_i32_be()?);
                    }
                    Ok(NbtTag::List(NbtList::Int(vec)))
                }
                TAG_LONG => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_i64_be()?);
                    }
                    Ok(NbtTag::List(NbtList::Long(vec)))
                }
                TAG_FLOAT => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_f32_be()?);
                    }
                    Ok(NbtTag::List(NbtList::Float(vec)))
                }
                TAG_DOUBLE => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_f64_be()?);
                    }
                    Ok(NbtTag::List(NbtList::Double(vec)))
                }
                TAG_BYTE_ARRAY => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        let sub_len = reader.read_i32_be()?;
                        if sub_len < 0 {
                            return Err(NbtError::InvalidData(
                                "Negative byte array in list".into(),
                            ));
                        }
                        let sub_count = sub_len as usize;
                        if reader.remaining() < sub_count {
                            return Err(NbtError::InvalidData(format!(
                                "Byte array in list requires {sub_count} bytes, but only {} remain",
                                reader.remaining()
                            )));
                        }
                        vec.push(reader.read_exact(sub_count)?);
                    }
                    Ok(NbtTag::List(NbtList::ByteArray(vec)))
                }
                TAG_STRING => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(reader.read_str()?);
                    }
                    Ok(NbtTag::List(NbtList::String(vec)))
                }
                TAG_LIST => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        let tag = read_tag_payload(reader, TAG_LIST, depth + 1)?;
                        if let NbtTag::List(l) = tag {
                            vec.push(l);
                        }
                    }
                    Ok(NbtTag::List(NbtList::List(vec)))
                }
                TAG_COMPOUND => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        vec.push(read_compound_payload(reader, depth + 1)?);
                    }
                    Ok(NbtTag::List(NbtList::Compound(vec)))
                }
                TAG_INT_ARRAY => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        let sub_len = reader.read_i32_be()?;
                        if sub_len < 0 {
                            return Err(NbtError::InvalidData("Negative int array in list".into()));
                        }
                        let sub_count = sub_len as usize;
                        let byte_count = sub_count.checked_mul(4).ok_or_else(|| {
                            NbtError::InvalidData("Int array element length overflow".into())
                        })?;
                        if reader.remaining() < byte_count {
                            return Err(NbtError::InvalidData(format!(
                                "Int array in list requires {byte_count} bytes, but only {} remain",
                                reader.remaining()
                            )));
                        }
                        vec.push(reader.read_exact(byte_count)?);
                    }
                    Ok(NbtTag::List(NbtList::IntArray(vec)))
                }
                TAG_LONG_ARRAY => {
                    let mut vec = Vec::with_capacity(count);
                    for _ in 0..count {
                        let sub_len = reader.read_i32_be()?;
                        if sub_len < 0 {
                            return Err(NbtError::InvalidData(
                                "Negative long array in list".into(),
                            ));
                        }
                        let sub_count = sub_len as usize;
                        let byte_count = sub_count.checked_mul(8).ok_or_else(|| {
                            NbtError::InvalidData("Long array element length overflow".into())
                        })?;
                        if reader.remaining() < byte_count {
                            return Err(NbtError::InvalidData(format!(
                                "Long array in list requires {byte_count} bytes, but only {} remain",
                                reader.remaining()
                            )));
                        }
                        vec.push(reader.read_exact(byte_count)?);
                    }
                    Ok(NbtTag::List(NbtList::LongArray(vec)))
                }
                unknown => Err(NbtError::UnknownTagId(unknown)),
            }
        }
        TAG_COMPOUND => Ok(NbtTag::Compound(read_compound_payload(reader, depth)?)),
        TAG_INT_ARRAY => {
            let len = reader.read_i32_be()?;
            if len < 0 {
                return Err(NbtError::InvalidData(format!(
                    "Negative int array length: {len}"
                )));
            }
            let count = len as usize;
            let byte_count = count.checked_mul(4).ok_or_else(|| {
                NbtError::InvalidData(format!("Int array length {count} overflows usize"))
            })?;
            if reader.remaining() < byte_count {
                return Err(NbtError::InvalidData(format!(
                    "Int array length {count} requires {byte_count} bytes, but only {} remain",
                    reader.remaining()
                )));
            }
            let bytes = reader.read_exact(byte_count)?;
            Ok(NbtTag::IntArray(bytes))
        }
        TAG_LONG_ARRAY => {
            let len = reader.read_i32_be()?;
            if len < 0 {
                return Err(NbtError::InvalidData(format!(
                    "Negative long array length: {len}"
                )));
            }
            let count = len as usize;
            let byte_count = count.checked_mul(8).ok_or_else(|| {
                NbtError::InvalidData(format!("Long array length {count} overflows usize"))
            })?;
            if reader.remaining() < byte_count {
                return Err(NbtError::InvalidData(format!(
                    "Long array length {count} requires {byte_count} bytes, but only {} remain",
                    reader.remaining()
                )));
            }
            let bytes = reader.read_exact(byte_count)?;
            Ok(NbtTag::LongArray(bytes))
        }
        unknown => Err(NbtError::UnknownTagId(unknown)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_huge_length_returns_err_immediately() {
        let all_types = [
            TAG_BYTE,
            TAG_SHORT,
            TAG_INT,
            TAG_LONG,
            TAG_FLOAT,
            TAG_DOUBLE,
            TAG_BYTE_ARRAY,
            TAG_STRING,
            TAG_LIST,
            TAG_COMPOUND,
            TAG_INT_ARRAY,
            TAG_LONG_ARRAY,
        ];

        for &item_type in &all_types {
            let mut payload = Vec::new();
            payload.push(item_type);
            payload.extend_from_slice(&0x7FFF_FFFFi32.to_be_bytes());
            payload.extend_from_slice(&[1, 2, 3]);

            let mut reader = SliceReader::new(&payload);
            let result = read_tag_payload(&mut reader, TAG_LIST, 0);
            assert!(
                matches!(result, Err(NbtError::InvalidData(_))),
                "item_type {item_type} did not return InvalidData"
            );
        }
    }

    #[test]
    fn test_negative_list_length_returns_err() {
        let mut payload = Vec::new();
        payload.push(TAG_INT);
        payload.extend_from_slice(&(-1i32).to_be_bytes());

        let mut reader = SliceReader::new(&payload);
        let result = read_tag_payload(&mut reader, TAG_LIST, 0);
        assert!(matches!(result, Err(NbtError::InvalidData(_))));
    }

    #[test]
    fn test_truncated_string_length_returns_err() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&500u16.to_be_bytes());
        payload.extend_from_slice(b"short");

        let mut reader = SliceReader::new(&payload);
        let result = reader.read_str();
        assert!(matches!(result, Err(NbtError::InvalidData(_))));
    }

    #[test]
    fn test_truncated_byte_array_length_returns_err() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1000i32.to_be_bytes());
        payload.extend_from_slice(&[1, 2, 3]);

        let mut reader = SliceReader::new(&payload);
        let result = read_tag_payload(&mut reader, TAG_BYTE_ARRAY, 0);
        assert!(matches!(result, Err(NbtError::InvalidData(_))));
    }

    #[test]
    fn test_truncated_int_and_long_array_returns_err() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&100i32.to_be_bytes());
        payload.extend_from_slice(&[1, 2, 3, 4]);

        let mut reader = SliceReader::new(&payload);
        let result = read_tag_payload(&mut reader, TAG_INT_ARRAY, 0);
        assert!(matches!(result, Err(NbtError::InvalidData(_))));

        let mut payload2 = Vec::new();
        payload2.extend_from_slice(&100i32.to_be_bytes());
        payload2.extend_from_slice(&[1, 2, 3, 4]);

        let mut reader2 = SliceReader::new(&payload2);
        let result2 = read_tag_payload(&mut reader2, TAG_LONG_ARRAY, 0);
        assert!(matches!(result2, Err(NbtError::InvalidData(_))));
    }

    #[test]
    fn test_negative_array_lengths_return_err() {
        for &tag in &[TAG_BYTE_ARRAY, TAG_INT_ARRAY, TAG_LONG_ARRAY] {
            let mut payload = Vec::new();
            payload.extend_from_slice(&(-5i32).to_be_bytes());

            let mut reader = SliceReader::new(&payload);
            let result = read_tag_payload(&mut reader, tag, 0);
            assert!(matches!(result, Err(NbtError::InvalidData(_))));
        }
    }

    #[test]
    fn test_checked_cursor_math_avoids_overflow() {
        let buf = [1u8, 2, 3];
        let mut reader = SliceReader::new(&buf);
        let result = reader.read_exact(usize::MAX);
        assert!(matches!(result, Err(NbtError::UnexpectedEof)));
    }
}
