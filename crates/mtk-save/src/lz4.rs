use crate::error::SaveError;

/// Magic header bytes for `net.jpountz.lz4.LZ4BlockOutputStream` blocks.
const LZ4_BLOCK_MAGIC: &[u8; 8] = b"LZ4Block";
/// Size of each `LZ4Block` header in bytes (8 magic + 1 token + 3x4 LE ints).
const LZ4_BLOCK_HEADER_SIZE: usize = 21;
/// Compression method nibble: raw uncompressed block.
const METHOD_RAW: u8 = 0x10;
/// Compression method nibble: LZ4-compressed block.
const METHOD_LZ4: u8 = 0x20;
/// Minimum compression level encoded in the token low nibble (`1 << 10 == 1024`).
const COMPRESSION_LEVEL_BASE: u32 = 10;
/// Seed used by `net.jpountz.lz4.LZ4BlockOutputStream` for XXHash32 block checksums.
const LZ4_XXHASH_SEED: u32 = 0x9747_B28C;
/// Mask applied by `StreamingXXHash32.asChecksum()` (`0x0FFF_FFFF`).
const LZ4_CHECKSUM_MASK: u32 = 0x0FFF_FFFF;

/// Decompresses a Java `net.jpountz.lz4.LZ4BlockInputStream` byte stream up to `max_output_size`.
pub(crate) fn decompress_lz4_block_stream(
    input: &[u8],
    max_output_size: usize,
) -> Result<Vec<u8>, SaveError> {
    if input.is_empty() {
        return Err(SaveError::DecompressionFailed(
            "LZ4 stream ended prematurely (empty input)".to_string(),
        ));
    }

    let mut output = Vec::new();
    let mut pos = 0usize;

    while pos < input.len() {
        if input.len() - pos < LZ4_BLOCK_HEADER_SIZE {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 block header truncated at byte {pos}: expected {LZ4_BLOCK_HEADER_SIZE} bytes, got {}",
                input.len() - pos
            )));
        }

        let header = &input[pos..pos + LZ4_BLOCK_HEADER_SIZE];
        pos += LZ4_BLOCK_HEADER_SIZE;

        if &header[0..8] != LZ4_BLOCK_MAGIC {
            return Err(SaveError::DecompressionFailed(
                "LZ4 stream is corrupted (invalid block magic)".to_string(),
            ));
        }

        let token = header[8];
        let method = token & 0xF0;
        if method != METHOD_RAW && method != METHOD_LZ4 {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 stream has unsupported compression method: 0x{method:02X}"
            )));
        }
        let compression_level = COMPRESSION_LEVEL_BASE + (token & 0x0F) as u32;
        let max_block_size = 1usize << compression_level;

        let compressed_len_i32 =
            i32::from_le_bytes([header[9], header[10], header[11], header[12]]);
        let decompressed_len_i32 =
            i32::from_le_bytes([header[13], header[14], header[15], header[16]]);
        let checksum = u32::from_le_bytes([header[17], header[18], header[19], header[20]]);

        if compressed_len_i32 < 0 || decompressed_len_i32 < 0 {
            return Err(SaveError::DecompressionFailed(
                "LZ4 block has negative length".to_string(),
            ));
        }

        let compressed_len = compressed_len_i32 as usize;
        let decompressed_len = decompressed_len_i32 as usize;

        if decompressed_len > max_block_size {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 block decompressed length {decompressed_len} exceeds token block limit {max_block_size}"
            )));
        }

        if compressed_len == 0 && decompressed_len == 0 {
            if checksum != 0 {
                return Err(SaveError::DecompressionFailed(
                    "LZ4 end-of-stream marker has non-zero checksum".to_string(),
                ));
            }
            break;
        }

        if compressed_len == 0 || decompressed_len == 0 {
            return Err(SaveError::DecompressionFailed(
                "LZ4 block has inconsistent zero/non-zero lengths".to_string(),
            ));
        }

        if method == METHOD_RAW && compressed_len != decompressed_len {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 raw block length mismatch: compressed={compressed_len}, decompressed={decompressed_len}"
            )));
        }

        let next_total = output.len().checked_add(decompressed_len).ok_or_else(|| {
            SaveError::DecompressionFailed("LZ4 output size overflow".to_string())
        })?;
        if next_total > max_output_size {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 decompressed stream exceeded {max_output_size} byte limit (needed {next_total} bytes)"
            )));
        }

        if input.len() - pos < compressed_len {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 block payload truncated: expected {compressed_len} bytes, got {}",
                input.len() - pos
            )));
        }

        let payload = &input[pos..pos + compressed_len];
        pos += compressed_len;

        let block_start = output.len();
        output.reserve(decompressed_len);

        match method {
            METHOD_RAW => output.extend_from_slice(payload),
            METHOD_LZ4 => decode_raw_lz4_block(payload, decompressed_len, &mut output)?,
            _ => unreachable!("method validated above"),
        }

        let actual_checksum = xxh32(&output[block_start..], LZ4_XXHASH_SEED) & LZ4_CHECKSUM_MASK;
        if actual_checksum != checksum {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 block checksum mismatch: expected 0x{checksum:08X}, computed 0x{actual_checksum:08X}"
            )));
        }
    }

    Ok(output)
}

/// Decodes a single raw LZ4 block of expected uncompressed size `expected_len` into `output`.
fn decode_raw_lz4_block(
    src: &[u8],
    expected_len: usize,
    output: &mut Vec<u8>,
) -> Result<(), SaveError> {
    let block_start = output.len();
    let mut s = 0usize;

    while s < src.len() {
        let token = src[s];
        s += 1;

        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            lit_len = read_extended_length(src, &mut s, lit_len)?;
        }

        if src.len() - s < lit_len {
            return Err(SaveError::DecompressionFailed(
                "LZ4 block truncated inside literal run".to_string(),
            ));
        }

        let written = output.len() - block_start;
        if written
            .checked_add(lit_len)
            .is_none_or(|n| n > expected_len)
        {
            return Err(SaveError::DecompressionFailed(
                "LZ4 literal run exceeds declared decompressed block length".to_string(),
            ));
        }

        output.extend_from_slice(&src[s..s + lit_len]);
        s += lit_len;

        // The last sequence of a valid LZ4 block has literals only and stops at end-of-block.
        if s == src.len() {
            break;
        }

        if src.len() - s < 2 {
            return Err(SaveError::DecompressionFailed(
                "LZ4 block truncated inside match offset".to_string(),
            ));
        }
        let offset = u16::from_le_bytes([src[s], src[s + 1]]) as usize;
        s += 2;

        let block_written = output.len() - block_start;
        if offset == 0 || offset > block_written {
            return Err(SaveError::DecompressionFailed(format!(
                "LZ4 invalid match offset {offset} at block position {block_written}"
            )));
        }

        let mut match_len = (token & 0x0F) as usize;
        if match_len == 15 {
            match_len = read_extended_length(src, &mut s, match_len)?;
        }
        match_len = match_len.checked_add(4).ok_or_else(|| {
            SaveError::DecompressionFailed("LZ4 match length overflow".to_string())
        })?;

        if block_written
            .checked_add(match_len)
            .is_none_or(|n| n > expected_len)
        {
            return Err(SaveError::DecompressionFailed(
                "LZ4 match exceeds declared decompressed block length".to_string(),
            ));
        }

        // Byte-by-byte copy handles overlapping matches (including RLE with offset == 1).
        for _ in 0..match_len {
            let b = output[output.len() - offset];
            output.push(b);
        }
    }

    let decoded_len = output.len() - block_start;
    if decoded_len != expected_len {
        return Err(SaveError::DecompressionFailed(format!(
            "LZ4 block decoded length mismatch: expected {expected_len}, got {decoded_len}"
        )));
    }

    Ok(())
}

fn read_extended_length(src: &[u8], s: &mut usize, mut len: usize) -> Result<usize, SaveError> {
    loop {
        if *s >= src.len() {
            return Err(SaveError::DecompressionFailed(
                "LZ4 block truncated while reading extended length".to_string(),
            ));
        }
        let extra = src[*s] as usize;
        *s += 1;
        len = len.checked_add(extra).ok_or_else(|| {
            SaveError::DecompressionFailed("LZ4 extended length overflow".to_string())
        })?;
        if extra != 255 {
            return Ok(len);
        }
    }
}

/// Computes standard 32-bit XXHash (`XXH32`) of `data` with `seed`.
pub(crate) fn xxh32(data: &[u8], seed: u32) -> u32 {
    const P1: u32 = 0x9E37_79B1;
    const P2: u32 = 0x85EB_CA77;
    const P3: u32 = 0xC2B2_AE3D;
    const P4: u32 = 0x27D4_EB2F;
    const P5: u32 = 0x1656_67B1;

    let mut h32: u32;
    let mut i = 0usize;

    if data.len() >= 16 {
        let mut v1 = seed.wrapping_add(P1).wrapping_add(P2);
        let mut v2 = seed.wrapping_add(P2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(P1);

        let limit = data.len() - 16;
        while i <= limit {
            v1 = xxh32_round(
                v1,
                u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]),
            );
            v2 = xxh32_round(
                v2,
                u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]),
            );
            v3 = xxh32_round(
                v3,
                u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]),
            );
            v4 = xxh32_round(
                v4,
                u32::from_le_bytes([data[i + 12], data[i + 13], data[i + 14], data[i + 15]]),
            );
            i += 16;
        }

        h32 = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
    } else {
        h32 = seed.wrapping_add(P5);
    }

    h32 = h32.wrapping_add(data.len() as u32);

    while i + 4 <= data.len() {
        let k1 = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        h32 = h32
            .wrapping_add(k1.wrapping_mul(P3))
            .rotate_left(17)
            .wrapping_mul(P4);
        i += 4;
    }

    while i < data.len() {
        h32 = h32
            .wrapping_add((data[i] as u32).wrapping_mul(P5))
            .rotate_left(11)
            .wrapping_mul(P1);
        i += 1;
    }

    h32 ^= h32 >> 15;
    h32 = h32.wrapping_mul(P2);
    h32 ^= h32 >> 13;
    h32 = h32.wrapping_mul(P3);
    h32 ^= h32 >> 16;
    h32
}

#[inline]
fn xxh32_round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(0x85EB_CA77))
        .rotate_left(13)
        .wrapping_mul(0x9E37_79B1)
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;

    /// Encodes `payload` into a `LZ4BlockOutputStream`-compatible RAW block stream with end marker.
    pub(crate) fn encode_raw_lz4_stream(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if !payload.is_empty() {
            append_block(&mut out, METHOD_RAW | 0x06, payload, payload);
        }
        append_terminator(&mut out, 0x06);
        out
    }

    pub(crate) fn append_block(out: &mut Vec<u8>, token: u8, compressed: &[u8], raw: &[u8]) {
        let checksum = xxh32(raw, LZ4_XXHASH_SEED) & LZ4_CHECKSUM_MASK;
        out.extend_from_slice(LZ4_BLOCK_MAGIC);
        out.push(token);
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(compressed);
    }

    pub(crate) fn append_terminator(out: &mut Vec<u8>, level_nibble: u8) {
        out.extend_from_slice(LZ4_BLOCK_MAGIC);
        out.push(METHOD_RAW | (level_nibble & 0x0F));
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_xxh32_reference_vectors() {
        // Standard XXH32 reference vectors (verified against net.jpountz.xxhash.XXHash32)
        assert_eq!(xxh32(b"", 0), 0x02CC_5D05);
        assert_eq!(xxh32(b"abc", 0), 0x32D1_53FF);
        assert_eq!(
            xxh32(b"Nobody inspects the spammish repetition", 0),
            0xE229_3B2F
        );
        assert_eq!(
            xxh32(b"Nobody inspects the spammish repetition", 0x9747_B28C),
            0x70B9_1719
        );
    }

    #[test]
    fn test_lz4_java_interop_fixture() {
        // Bytes emitted directly by net.jpountz.lz4.LZ4BlockOutputStream (lz4-java 1.8.0)
        const HEX: &str = "4c5a34426c6f636b263900000068010000024a6205ff0d4d696e656372616674204c5a342072657065617420746573743a204101000cdf2068656c6c6f20776f726c64214800ff09506f726c64214c5a34426c6f636b16000000000000000000000000";
        let bytes: Vec<u8> = (0..HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).unwrap())
            .collect();
        let expected =
            b"Minecraft LZ4 repeat test: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA hello world!".repeat(5);
        let decoded = decompress_lz4_block_stream(&bytes, 1024).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_lz4_raw_block_and_end_marker() {
        let payload = b"Hello, Minecraft LZ4 world!";
        let stream = encode_raw_lz4_stream(payload);
        let decoded = decompress_lz4_block_stream(&stream, 1024).unwrap();
        assert_eq!(decoded, payload);

        // Empty payload (terminator only) decodes to empty Vec
        let empty_stream = encode_raw_lz4_stream(&[]);
        let decoded_empty = decompress_lz4_block_stream(&empty_stream, 1024).unwrap();
        assert!(decoded_empty.is_empty());
    }

    #[test]
    fn test_lz4_compressed_block_rle_backref_and_extended_length() {
        // Construct raw LZ4 payload encoding:
        // 1) 17 literals: b"ABCDEFGHIJKLMNOPQ" (lit_len = 15 + 2)
        //    followed by RLE match of offset=1, match_len = 4 + 15 + 1 = 20 -> 20 x 'Q'
        // 2) trailing 3 literals: b"XYZ" (last sequence, no match)
        let mut expected = b"ABCDEFGHIJKLMNOPQ".to_vec();
        expected.extend(std::iter::repeat_n(b'Q', 20));
        expected.extend_from_slice(b"XYZ");

        let mut compressed = Vec::new();
        // Token 1: lit=15 (extended), match=15 (extended -> 4+15+1 = 20)
        compressed.push(0xFF);
        compressed.push(2); // 15 + 2 = 17 literals
        compressed.extend_from_slice(b"ABCDEFGHIJKLMNOPQ");
        compressed.extend_from_slice(&1u16.to_le_bytes()); // offset = 1 (overlapping RLE)
        compressed.push(1); // 4 + 15 + 1 = 20 match bytes
                            // Token 2 (final sequence): lit=3, match=0
        compressed.push(0x30);
        compressed.extend_from_slice(b"XYZ");

        let mut stream = Vec::new();
        append_block(&mut stream, METHOD_LZ4 | 0x06, &compressed, &expected);
        append_terminator(&mut stream, 0x06);

        let decoded = decompress_lz4_block_stream(&stream, 1024).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_lz4_multi_block_stream() {
        let part1 = b"first block payload with enough bytes ";
        let mut part2_expected = b"ABCD".to_vec();
        part2_expected.extend_from_slice(b"ABCDABCD"); // 8 bytes via back-reference offset=4, match=8

        let mut part2_compressed = Vec::new();
        // Token: lit=4, match=8-4=4 -> 0x44
        part2_compressed.push(0x44);
        part2_compressed.extend_from_slice(b"ABCD");
        part2_compressed.extend_from_slice(&4u16.to_le_bytes());
        // Last sequence: empty literal? No, in LZ4 every block ends after literals if s == len;
        // or wait: if last sequence has a match, `s` reached `src.len()`? No, after match we loop,
        // Wait: in LZ4 a block's last 5 bytes MUST be literals (spec requires last sequence has no match),
        // let's append a final literal token for b"!" so it's strictly valid LZ4.
        part2_expected.push(b'!');
        part2_compressed.push(0x10);
        part2_compressed.push(b'!');

        let mut stream = Vec::new();
        append_block(&mut stream, METHOD_RAW | 0x06, part1, part1);
        append_block(
            &mut stream,
            METHOD_LZ4 | 0x06,
            &part2_compressed,
            &part2_expected,
        );
        append_terminator(&mut stream, 0x06);

        let decoded = decompress_lz4_block_stream(&stream, 4096).unwrap();
        let mut expected = part1.to_vec();
        expected.extend_from_slice(&part2_expected);
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_lz4_error_cases() {
        // 1. Invalid magic
        let mut bad_magic = encode_raw_lz4_stream(b"hello");
        bad_magic[0] = b'X';
        assert!(matches!(
            decompress_lz4_block_stream(&bad_magic, 1024),
            Err(SaveError::DecompressionFailed(_))
        ));

        // 2. Invalid checksum
        let mut bad_crc = encode_raw_lz4_stream(b"hello");
        bad_crc[17] ^= 0x01;
        assert!(matches!(
            decompress_lz4_block_stream(&bad_crc, 1024),
            Err(SaveError::DecompressionFailed(_))
        ));

        // 3. Truncated stream
        let valid = encode_raw_lz4_stream(b"hello world");
        assert!(matches!(
            decompress_lz4_block_stream(&valid[..valid.len() - 25], 1024),
            Err(SaveError::DecompressionFailed(_))
        ));

        // 4. Decompression bomb exceeding max_output_size
        // Encode a 128-byte RLE block using ~8 compressed bytes, cap at 64 bytes
        let bomb_raw = vec![b'A'; 128];
        let mut bomb_comp = Vec::new();
        bomb_comp.push(0x1F); // 1 lit, match = 15 + extra
        bomb_comp.push(b'A');
        bomb_comp.extend_from_slice(&1u16.to_le_bytes());
        bomb_comp.push(126 - 4 - 15); // match = 126
        bomb_comp.push(0x10); // 1 trailing literal
        bomb_comp.push(b'A');

        let mut bomb_stream = Vec::new();
        append_block(&mut bomb_stream, METHOD_LZ4 | 0x06, &bomb_comp, &bomb_raw);
        append_terminator(&mut bomb_stream, 0x06);

        // First verify it decodes when cap is sufficient
        assert_eq!(
            decompress_lz4_block_stream(&bomb_stream, 256).unwrap(),
            bomb_raw
        );
        // Then verify cap rejects it
        assert!(matches!(
            decompress_lz4_block_stream(&bomb_stream, 64),
            Err(SaveError::DecompressionFailed(_))
        ));
    }
}
