//! # LSB-First Bit Packing and Unpacking (RFC v3.1 Section 4.4)
//!
//! Provides canonical primitives for packing and unpacking indices into
//! bit streams with support for 1, 2, 4, 8, and 16 bits per item.

/// Selects the minimum bits per item needed to represent indices for a given palette length.
#[inline]
pub fn choose_bits_per_item(palette_len: usize) -> u8 {
    if palette_len <= 2 {
        1
    } else if palette_len <= 4 {
        2
    } else if palette_len <= 16 {
        4
    } else if palette_len <= 256 {
        8
    } else {
        16
    }
}

/// Packs u16 indices into an LSB-first bitstream byte buffer.
pub fn pack_lsb_indices(indices: &[u16], bits_per_item: u8) -> Vec<u8> {
    if indices.is_empty() {
        return Vec::new();
    }

    match bits_per_item {
        8 => indices.iter().map(|&v| v as u8).collect(),
        16 => {
            let mut bytes = Vec::with_capacity(indices.len() * 2);
            for &v in indices {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes
        }
        4 => {
            let num_bytes = indices.len().div_ceil(2);
            let mut bytes = Vec::with_capacity(num_bytes);
            for chunk in indices.chunks(2) {
                let b0 = (chunk[0] & 0x0F) as u8;
                let b1 = if chunk.len() > 1 {
                    (chunk[1] & 0x0F) as u8
                } else {
                    0
                };
                bytes.push(b0 | (b1 << 4));
            }
            bytes
        }
        2 => {
            let num_bytes = indices.len().div_ceil(4);
            let mut bytes = Vec::with_capacity(num_bytes);
            for chunk in indices.chunks(4) {
                let mut b = 0u8;
                for (slot, &val) in chunk.iter().enumerate() {
                    b |= ((val & 0x03) as u8) << (slot * 2);
                }
                bytes.push(b);
            }
            bytes
        }
        1 => {
            let num_bytes = indices.len().div_ceil(8);
            let mut bytes = Vec::with_capacity(num_bytes);
            for chunk in indices.chunks(8) {
                let mut b = 0u8;
                for (slot, &val) in chunk.iter().enumerate() {
                    b |= ((val & 0x01) as u8) << slot;
                }
                bytes.push(b);
            }
            bytes
        }
        _ => {
            // Default fallback to 16-bit LE
            let mut bytes = Vec::with_capacity(indices.len() * 2);
            for &v in indices {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes
        }
    }
}

/// Unpacks LSB-first bit-packed bytes into a u16 vector.
pub fn unpack_lsb_indices(bytes: &[u8], count: usize, bits_per_item: u8) -> Vec<u16> {
    if count == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);

    match bits_per_item {
        8 => {
            for &b in bytes.iter().take(count) {
                out.push(b as u16);
            }
        }
        16 => {
            let limit = count.min(bytes.len() / 2);
            for i in 0..limit {
                let val = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
                out.push(val);
            }
        }
        1 => {
            let mut extracted = 0;
            for &byte in bytes {
                for bit in 0..8 {
                    if extracted >= count {
                        break;
                    }
                    out.push(((byte >> bit) & 0x01) as u16);
                    extracted += 1;
                }
                if extracted >= count {
                    break;
                }
            }
        }
        2 => {
            let mut extracted = 0;
            for &byte in bytes {
                for slot in 0..4 {
                    if extracted >= count {
                        break;
                    }
                    out.push(((byte >> (slot * 2)) & 0x03) as u16);
                    extracted += 1;
                }
                if extracted >= count {
                    break;
                }
            }
        }
        4 => {
            let mut extracted = 0;
            for &byte in bytes {
                for slot in 0..2 {
                    if extracted >= count {
                        break;
                    }
                    out.push(((byte >> (slot * 4)) & 0x0F) as u16);
                    extracted += 1;
                }
                if extracted >= count {
                    break;
                }
            }
        }
        _ => {
            for &b in bytes.iter().take(count) {
                out.push(b as u16);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choose_bits_per_item() {
        assert_eq!(choose_bits_per_item(0), 1);
        assert_eq!(choose_bits_per_item(1), 1);
        assert_eq!(choose_bits_per_item(2), 1);
        assert_eq!(choose_bits_per_item(3), 2);
        assert_eq!(choose_bits_per_item(4), 2);
        assert_eq!(choose_bits_per_item(5), 4);
        assert_eq!(choose_bits_per_item(16), 4);
        assert_eq!(choose_bits_per_item(17), 8);
        assert_eq!(choose_bits_per_item(256), 8);
        assert_eq!(choose_bits_per_item(257), 16);
        assert_eq!(choose_bits_per_item(65535), 16);
    }

    #[test]
    fn test_bitpack_roundtrip_all_widths() {
        let test_cases: &[(u8, Vec<u16>)] = &[
            (1, vec![0, 1, 1, 0, 1, 0, 0, 1, 1, 1, 0]),
            (2, vec![0, 1, 2, 3, 2, 1, 0, 3, 1]),
            (4, vec![0, 5, 12, 15, 1, 7, 9, 14, 3]),
            (8, vec![0, 42, 100, 255, 13, 77, 199]),
            (16, vec![0, 300, 1024, 65534, 42, 10000]),
        ];

        for &(bits, ref indices) in test_cases {
            let packed = pack_lsb_indices(indices, bits);
            let unpacked = unpack_lsb_indices(&packed, indices.len(), bits);
            assert_eq!(
                &unpacked, indices,
                "Roundtrip mismatch for bits_per_item={bits}"
            );
        }
    }
}
