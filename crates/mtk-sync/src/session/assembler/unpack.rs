/// Helper to unpack LSB-first bit-packed indices into u16 vector.
pub(crate) fn unpack_lsb_indices(bytes: &[u8], count: usize, bits_per_item: u8) -> Vec<u16> {
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
            // Fallback: unpack as byte-aligned if unrecognized bit width
            for &b in bytes.iter().take(count) {
                out.push(b as u16);
            }
        }
    }

    out
}
