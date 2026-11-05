/// Standard IEEE 802.3 / ISO 3309 CRC32 lookup table (polynomial 0xEDB88320).
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Updates a running IEEE 802.3 CRC32 checksum with a byte slice.
#[inline]
pub(crate) fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    crc = !crc;
    for &b in data {
        crc = CRC32_TABLE[((crc ^ (b as u32)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Precomputed empty air section lookup table for 0..=4096 consecutive "minecraft:air" voxels.
static EMPTY_CRC_TABLE: [u32; 4097] = {
    let mut table = [0u32; 4097];
    let air_bytes = b"minecraft:air";
    let mut crc_val = 0u32;
    table[0] = 0;
    let mut count = 1;
    while count <= 4096 {
        // Compute crc32_update(crc_val, air_bytes)
        let mut crc = !crc_val;
        let mut bi = 0;
        while bi < air_bytes.len() {
            let b = air_bytes[bi];
            crc = CRC32_TABLE[((crc ^ (b as u32)) & 0xFF) as usize] ^ (crc >> 8);
            bi += 1;
        }
        crc_val = !crc;
        table[count] = crc_val;
        count += 1;
    }
    table
};

/// Returns the canonical CRC32 for an empty chunk/section of air with `block_count` blocks.
#[inline]
pub(crate) fn get_empty_section_crc(block_count: usize) -> u32 {
    if block_count <= 4096 {
        EMPTY_CRC_TABLE[block_count]
    } else {
        let air_bytes = b"minecraft:air";
        let mut crc_val = 0u32;
        for _ in 0..block_count {
            crc_val = crc32_update(crc_val, air_bytes);
        }
        crc_val
    }
}

/// Canonical CRC32 for a standard full 16x16x16 empty air section (4096 air voxels).
pub(crate) const EMPTY_SECTION_CRC: u32 = EMPTY_CRC_TABLE[4096];

/// Extracts canonical Minecraft blockstate identifier from raw state or JSON-wrapped string.
/// Converts variants of air / structure_void / bubble_column to canonical "minecraft:air".
#[inline]
pub(crate) fn extract_canonical_state_str(raw_state: &str) -> &str {
    if raw_state.is_empty() || raw_state == "minecraft:air" {
        return "minecraft:air";
    }

    let mut res = raw_state;
    if let Some(stripped) = raw_state.strip_prefix("{\"state\":\"") {
        if let Some(end_idx) = stripped.find('"') {
            res = &stripped[..end_idx];
        }
    }

    // Property-insensitive family matching, mirroring the Java `BlockState.is(...)` SSOT.
    let base = res
        .find('[')
        .map_or(res, |property_start| &res[..property_start]);
    match base {
        "minecraft:air"
        | "minecraft:cave_air"
        | "minecraft:void_air"
        | "air"
        | "cave_air"
        | "void_air"
        | "structure_void"
        | "minecraft:structure_void"
        | "bubble_column"
        | "minecraft:bubble_column" => "minecraft:air",
        _ => res,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_basic() {
        assert_eq!(crc32_update(0, b""), 0);
        let crc_air = crc32_update(0, b"minecraft:air");
        assert_ne!(crc_air, 0);

        let crc_two_air = crc32_update(crc_air, b"minecraft:air");
        assert_eq!(EMPTY_CRC_TABLE[2], crc_two_air);
        assert_eq!(get_empty_section_crc(4096), EMPTY_SECTION_CRC);
    }

    #[test]
    fn test_extract_canonical_state() {
        assert_eq!(extract_canonical_state_str(""), "minecraft:air");
        assert_eq!(
            extract_canonical_state_str("minecraft:cave_air"),
            "minecraft:air"
        );
        assert_eq!(
            extract_canonical_state_str("minecraft:stone"),
            "minecraft:stone"
        );
        assert_eq!(
            extract_canonical_state_str("{\"state\":\"minecraft:dirt\"}"),
            "minecraft:dirt"
        );
    }
}
