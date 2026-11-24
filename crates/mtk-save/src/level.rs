use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::nbt::Nbt;
use flate2::read::GzDecoder;
use glam::IVec3;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::SaveError;

/// Maximum allowed size of `level.dat` (raw or decompressed, 64 MiB).
const MAX_LEVEL_DAT_SIZE: usize = 64 * 1024 * 1024;

/// World metadata extracted from `level.dat`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LevelData {
    /// Human-readable world name.
    pub level_name: String,
    /// Game version name string (e.g. "1.20.4", "1.21.1").
    pub version_name: String,
    /// DataVersion integer (e.g. 2844 for 1.18, 3700+ for 1.20.4+).
    pub data_version: i32,
    /// World spawn coordinates `[X, Y, Z]`.
    pub spawn: IVec3,
    /// World age in ticks.
    pub time: i64,
    /// Time of day in ticks (`0..24000`).
    pub day_time: i64,
    /// Whether the world is running in Hardcore mode.
    pub hardcore: bool,
    /// Default game mode index (0=Survival, 1=Creative, 2=Adventure, 3=Spectator).
    pub game_type: i32,
}

impl LevelData {
    /// Reads and parses `level.dat` (GZip-compressed or uncompressed NBT) from a file path.
    pub(crate) fn read_from_file(file_path: &Path) -> Result<Self, SaveError> {
        if !file_path.exists() {
            return Err(SaveError::LevelDatNotFound(file_path.to_path_buf()));
        }

        let file = File::open(file_path)?;
        let mut raw = Vec::new();
        file.take((MAX_LEVEL_DAT_SIZE as u64) + 1)
            .read_to_end(&mut raw)?;
        if raw.len() > MAX_LEVEL_DAT_SIZE {
            return Err(SaveError::InvalidChunkData(format!(
                "level.dat exceeded {MAX_LEVEL_DAT_SIZE} byte limit"
            )));
        }

        if raw.starts_with(&[0x1F, 0x8B]) {
            let gz = GzDecoder::new(&raw[..]);
            let mut decompressed = Vec::new();
            gz.take((MAX_LEVEL_DAT_SIZE as u64) + 1)
                .read_to_end(&mut decompressed)
                .map_err(|e| {
                    SaveError::DecompressionFailed(format!("Failed decompressing level.dat: {e}"))
                })?;
            if decompressed.len() > MAX_LEVEL_DAT_SIZE {
                return Err(SaveError::DecompressionFailed(format!(
                    "Decompressed level.dat exceeded {MAX_LEVEL_DAT_SIZE} byte limit"
                )));
            }
            Self::read_from_bytes(&decompressed)
        } else {
            Self::read_from_bytes(&raw)
        }
    }

    /// Parses decompressed bytes of `level.dat` via lightweight NBT decoder.
    pub(crate) fn read_from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let nbt = crate::nbt::read(&mut Cursor::new(bytes))?;
        let base = match nbt {
            Nbt::Some(b) => b,
            Nbt::None => {
                return Err(SaveError::InvalidChunkData(
                    "level.dat contains empty NBT".to_string(),
                ))
            }
        };

        let root = base.as_compound();
        let data = root.compound("Data").ok_or_else(|| {
            SaveError::InvalidChunkData("Missing 'Data' compound in level.dat".to_string())
        })?;

        let level_name = data
            .string("LevelName")
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Minecraft World".to_string());

        let version_name = data
            .compound("Version")
            .and_then(|v| v.string("Name"))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown".to_string());

        let data_version = data.int("DataVersion").unwrap_or(0);

        // 1.21.9+/26.x stores spawn inside `Data.spawn.pos`, while classic saves use `SpawnX/Y/Z`.
        let spawn = data
            .compound("spawn")
            .and_then(|s| s.int_array("pos"))
            .filter(|arr| arr.len() >= 3)
            .map(|arr| IVec3::new(arr[0], arr[1], arr[2]))
            .unwrap_or_else(|| {
                let spawn_x = data.int("SpawnX").unwrap_or(0);
                let spawn_y = data.int("SpawnY").unwrap_or(64);
                let spawn_z = data.int("SpawnZ").unwrap_or(0);
                IVec3::new(spawn_x, spawn_y, spawn_z)
            });

        let time = data.long("Time").unwrap_or(0);
        let day_time = data.long("DayTime").unwrap_or(0);
        let hardcore = data.byte("hardcore").unwrap_or(0) != 0;
        let game_type = data.int("GameType").unwrap_or(0);

        Ok(Self {
            level_name,
            version_name,
            data_version,
            spawn,
            time,
            day_time,
            hardcore,
            game_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::*;

    fn push_tag_header(buf: &mut Vec<u8>, tag_type: u8, name: &str) {
        buf.push(tag_type);
        buf.extend_from_slice(&(name.len() as u16).to_be_bytes());
        buf.extend_from_slice(name.as_bytes());
    }

    fn push_string_tag(buf: &mut Vec<u8>, name: &str, value: &str) {
        push_tag_header(buf, TAG_STRING, name);
        buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    fn push_int_tag(buf: &mut Vec<u8>, name: &str, value: i32) {
        push_tag_header(buf, TAG_INT, name);
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn build_level_dat_nbt(
        modern_spawn: Option<[i32; 3]>,
        classic_spawn: Option<[i32; 3]>,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        push_tag_header(&mut buf, TAG_COMPOUND, "");
        push_tag_header(&mut buf, TAG_COMPOUND, "Data");
        push_string_tag(&mut buf, "LevelName", "Test World");
        push_int_tag(&mut buf, "DataVersion", 4903);

        push_tag_header(&mut buf, TAG_COMPOUND, "Version");
        push_string_tag(&mut buf, "Name", "26.2");
        buf.push(TAG_END);

        if let Some(pos) = modern_spawn {
            push_tag_header(&mut buf, TAG_COMPOUND, "spawn");
            push_string_tag(&mut buf, "dimension", "minecraft:overworld");
            push_tag_header(&mut buf, TAG_INT_ARRAY, "pos");
            buf.extend_from_slice(&3i32.to_be_bytes());
            for v in pos {
                buf.extend_from_slice(&v.to_be_bytes());
            }
            buf.push(TAG_END);
        }

        if let Some([sx, sy, sz]) = classic_spawn {
            push_int_tag(&mut buf, "SpawnX", sx);
            push_int_tag(&mut buf, "SpawnY", sy);
            push_int_tag(&mut buf, "SpawnZ", sz);
        }

        buf.push(TAG_END); // Data
        buf.push(TAG_END); // root
        buf
    }

    #[test]
    fn test_read_level_dat_classic_and_modern_spawn() {
        let classic_bytes = build_level_dat_nbt(None, Some([12, 72, -34]));
        let classic = LevelData::read_from_bytes(&classic_bytes).unwrap();
        assert_eq!(classic.level_name, "Test World");
        assert_eq!(classic.version_name, "26.2");
        assert_eq!(classic.data_version, 4903);
        assert_eq!(classic.spawn, IVec3::new(12, 72, -34));

        let modern_bytes = build_level_dat_nbt(Some([-120, 95, 440]), None);
        let modern = LevelData::read_from_bytes(&modern_bytes).unwrap();
        assert_eq!(modern.spawn, IVec3::new(-120, 95, 440));
    }

    #[test]
    fn test_read_level_dat_gzip_and_uncompressed_files() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nbt = build_level_dat_nbt(Some([7, 80, -9]), None);
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(&nbt).unwrap();
        let gz_bytes = gz.finish().unwrap();

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mtk_level_{}_{}", std::process::id(), unique));
        std::fs::create_dir_all(&dir).unwrap();

        let gz_path = dir.join("level_gz.dat");
        let raw_path = dir.join("level_raw.dat");
        std::fs::write(&gz_path, &gz_bytes).unwrap();
        std::fs::write(&raw_path, &nbt).unwrap();

        let from_gz = LevelData::read_from_file(&gz_path).unwrap();
        let from_raw = LevelData::read_from_file(&raw_path).unwrap();
        assert_eq!(from_gz.spawn, IVec3::new(7, 80, -9));
        assert_eq!(from_raw.spawn, IVec3::new(7, 80, -9));
        assert_eq!(from_gz, from_raw);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
