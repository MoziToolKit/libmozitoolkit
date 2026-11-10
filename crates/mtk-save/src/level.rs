use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::nbt::Nbt;
use flate2::read::GzDecoder;
use glam::IVec3;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::SaveError;

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
    /// Reads and parses `level.dat` from a file path.
    pub(crate) fn read_from_file(file_path: &Path) -> Result<Self, SaveError> {
        if !file_path.exists() {
            return Err(SaveError::LevelDatNotFound(file_path.to_path_buf()));
        }

        let file = File::open(file_path)?;
        let mut gz = GzDecoder::new(file);
        let mut decompressed = Vec::new();
        gz.read_to_end(&mut decompressed).map_err(|e| {
            SaveError::DecompressionFailed(format!("Failed decompressing level.dat: {}", e))
        })?;

        Self::read_from_bytes(&decompressed)
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

        let spawn_x = data.int("SpawnX").unwrap_or(0);
        let spawn_y = data.int("SpawnY").unwrap_or(64);
        let spawn_z = data.int("SpawnZ").unwrap_or(0);

        let time = data.long("Time").unwrap_or(0);
        let day_time = data.long("DayTime").unwrap_or(0);
        let hardcore = data.byte("hardcore").unwrap_or(0) != 0;
        let game_type = data.int("GameType").unwrap_or(0);

        Ok(Self {
            level_name,
            version_name,
            data_version,
            spawn: IVec3::new(spawn_x, spawn_y, spawn_z),
            time,
            day_time,
            hardcore,
            game_type,
        })
    }
}
