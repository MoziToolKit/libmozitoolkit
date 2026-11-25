//! Zstd compression/decompression abstraction (native `zstd` vs pure-Rust `ruzstd` on wasm32).

use std::io::{Cursor, Read};

use crate::error::{PackageError, PackageResult};

pub(crate) fn decompress_zstd_limited(
    compressed: &[u8],
    limit: u64,
    identifier: &str,
    initial_cap: usize,
) -> PackageResult<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    let mut decoder = zstd::Decoder::new(Cursor::new(compressed)).map_err(|e| {
        PackageError::DecompressionFailed {
            identifier: identifier.to_string(),
            source: e,
        }
    })?;

    #[cfg(target_arch = "wasm32")]
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(Cursor::new(compressed)).map_err(|e| {
            PackageError::DecompressionFailed {
                identifier: identifier.to_string(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            }
        })?;

    let mut out = Vec::with_capacity(initial_cap);
    let mut limited = (&mut decoder).take(limit + 1);
    limited
        .read_to_end(&mut out)
        .map_err(|e| PackageError::DecompressionFailed {
            identifier: identifier.to_string(),
            source: e,
        })?;
    if out.len() as u64 > limit {
        return Err(PackageError::DecompressionFailed {
            identifier: identifier.to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Decompressed payload '{}' exceeds limit of {} bytes",
                    identifier, limit
                ),
            ),
        });
    }
    Ok(out)
}

#[allow(unused_variables)]
pub(crate) fn compress_zstd(
    decompressed: &[u8],
    level: i32,
    identifier: &str,
) -> PackageResult<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        zstd::encode_all(Cursor::new(decompressed), level).map_err(|e| {
            PackageError::CompressionFailed {
                identifier: identifier.to_string(),
                source: e,
            }
        })
    }

    #[cfg(target_arch = "wasm32")]
    {
        Err(PackageError::CompressionFailed {
            identifier: identifier.to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Zstd compression is not supported on wasm32",
            ),
        })
    }
}
