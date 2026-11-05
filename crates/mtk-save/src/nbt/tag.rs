use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

/// A lightweight, zero-copy borrowed NBT string.
///
/// Borrows `&'a str` whenever the input is valid UTF-8, or falls back to
/// owned `Cow<'a, str>` for lossy conversions if encountering non-standard MUTF-8.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtStr<'a>(pub Cow<'a, str>);

impl<'a> NbtStr<'a> {
    #[cfg(test)]
    #[inline]
    pub fn from_borrowed(s: &'a str) -> Self {
        Self(Cow::Borrowed(s))
    }

    #[cfg(test)]
    #[inline]
    pub fn from_owned(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    pub fn to_string_lossy(&self) -> Cow<'a, str> {
        self.0.clone()
    }
}

impl<'a> Deref for NbtStr<'a> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> AsRef<str> for NbtStr<'a> {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'a> PartialEq<&str> for NbtStr<'a> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a> PartialEq<str> for NbtStr<'a> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<'a> fmt::Display for NbtStr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'a> fmt::Debug for NbtStr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

/// A borrowed NBT value tag.
#[derive(Debug, Clone, PartialEq)]
pub enum NbtTag<'a> {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(&'a [u8]),
    String(NbtStr<'a>),
    List(NbtList<'a>),
    Compound(NbtCompound<'a>),
    IntArray(&'a [u8]),
    LongArray(&'a [u8]),
}

impl<'a> NbtTag<'a> {
    #[inline]
    pub fn byte(&self) -> Option<i8> {
        match self {
            Self::Byte(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub fn int(&self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub fn long(&self) -> Option<i64> {
        match self {
            Self::Long(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub fn string(&self) -> Option<NbtStr<'a>> {
        match self {
            Self::String(v) => Some(v.clone()),
            _ => None,
        }
    }

    #[inline]
    pub fn compound(&self) -> Option<&NbtCompound<'a>> {
        match self {
            Self::Compound(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn list(&self) -> Option<&NbtList<'a>> {
        match self {
            Self::List(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn int_array(&self) -> Option<Vec<i32>> {
        match self {
            Self::IntArray(bytes) => {
                let count = bytes.len() / 4;
                let mut vec = Vec::with_capacity(count);
                for i in 0..count {
                    let start = i * 4;
                    let chunk: [u8; 4] = bytes[start..start + 4].try_into().ok()?;
                    vec.push(i32::from_be_bytes(chunk));
                }
                Some(vec)
            }
            _ => None,
        }
    }

    #[inline]
    pub fn long_array(&self) -> Option<Vec<i64>> {
        match self {
            Self::LongArray(bytes) => {
                let count = bytes.len() / 8;
                let mut vec = Vec::with_capacity(count);
                for i in 0..count {
                    let start = i * 8;
                    let chunk: [u8; 8] = bytes[start..start + 8].try_into().ok()?;
                    vec.push(i64::from_be_bytes(chunk));
                }
                Some(vec)
            }
            _ => None,
        }
    }
}

/// An NBT compound tag mapping string keys to arbitrary tag values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NbtCompound<'a> {
    pub(crate) entries: Vec<(NbtStr<'a>, NbtTag<'a>)>,
}

impl<'a> NbtCompound<'a> {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&NbtTag<'a>> {
        self.entries
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    #[inline]
    pub fn byte(&self, key: &str) -> Option<i8> {
        self.get(key).and_then(|t| t.byte())
    }

    #[inline]
    pub fn int(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|t| t.int())
    }

    #[inline]
    pub fn long(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|t| t.long())
    }

    #[inline]
    pub fn string(&self, key: &str) -> Option<NbtStr<'a>> {
        self.get(key).and_then(|t| t.string())
    }

    #[inline]
    pub fn compound(&self, key: &str) -> Option<&NbtCompound<'a>> {
        self.get(key).and_then(|t| t.compound())
    }

    #[inline]
    pub fn list(&self, key: &str) -> Option<&NbtList<'a>> {
        self.get(key).and_then(|t| t.list())
    }

    #[inline]
    pub fn int_array(&self, key: &str) -> Option<Vec<i32>> {
        self.get(key).and_then(|t| t.int_array())
    }

    #[inline]
    pub fn long_array(&self, key: &str) -> Option<Vec<i64>> {
        self.get(key).and_then(|t| t.long_array())
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&NbtStr<'a>, &NbtTag<'a>)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

/// An NBT list tag containing homogeneous elements.
#[derive(Debug, Clone, PartialEq)]
pub enum NbtList<'a> {
    Byte(Vec<i8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Float(Vec<f32>),
    Double(Vec<f64>),
    ByteArray(Vec<&'a [u8]>),
    String(Vec<NbtStr<'a>>),
    List(Vec<NbtList<'a>>),
    Compound(Vec<NbtCompound<'a>>),
    IntArray(Vec<&'a [u8]>),
    LongArray(Vec<&'a [u8]>),
    Empty,
}

impl<'a> NbtList<'a> {
    #[inline]
    pub fn compounds(&self) -> Option<Vec<NbtCompound<'a>>> {
        match self {
            Self::Compound(v) => Some(v.clone()),
            _ => None,
        }
    }

    #[inline]
    pub fn strings(&self) -> Option<&[NbtStr<'a>]> {
        match self {
            Self::String(v) => Some(v),
            _ => None,
        }
    }
}
