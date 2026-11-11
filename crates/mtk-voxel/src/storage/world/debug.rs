//! # Pure-Code Minecraft Debug World Generator
//!
//! Replaces legacy pre-baked voxel snapshots with dynamic, programmatic generation
//! matching vanilla Minecraft's canonical `DebugLevelSource` algorithm:
//! - All block states are brute-force enumerated from available resource packs (or built-ins).
//! - Arranged on an even grid at `y = 70` with 1-block spacing (`x = 2 * i + 1`, `z = 2 * j + 1`).
//! - Grid bounds are computed as `GRID_WIDTH = ceil(sqrt(N))`, `GRID_HEIGHT = ceil(N / GRID_WIDTH)`.
//! - Supports modded blocks out-of-the-box via `ResourcePackStack`.

#[cfg(feature = "serde")]
use std::collections::HashSet;
use std::path::Path;
use std::sync::RwLock;

use crate::storage::VoxelStorage;
use crate::types::VoxelError;

/// Canonical Minecraft debug world block plane elevation.
pub const DEBUG_WORLD_Y: i32 = 70;

static CACHED_DEBUG_STORAGE: RwLock<Option<VoxelStorage>> = RwLock::new(None);

/// Returns true if the blockstate string represents any variant of air.
#[inline]
pub fn is_air_state(state: &str) -> bool {
    let name = state.split('[').next().unwrap_or(state);
    let short_name = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(short_name, "air" | "cave_air" | "void_air")
}

impl VoxelStorage {
    /// Pure-code generation of a canonical Minecraft debug world from an arbitrary slice of blockstate strings.
    ///
    /// Implements the exact grid layout from vanilla Minecraft `DebugLevelSource`:
    /// - `GRID_WIDTH = ceil(sqrt(N))`
    /// - `GRID_HEIGHT = ceil(N / GRID_WIDTH)`
    /// - Blocks are placed at `x = 2 * (index / GRID_WIDTH) + 1`, `y = 70`, `z = 2 * (index % GRID_WIDTH) + 1`
    /// - Blocks are spaced 1 block apart in both X and Z directions.
    /// - Air blocks are skipped.
    /// - Dynamic bounds encompass the entire generated field.
    pub fn create_debug_world_from_states<S: AsRef<str>>(states: &[S]) -> Result<Self, VoxelError> {
        let total = states.len();
        if total == 0 {
            return Err(VoxelError::MissingAssets(
                "Cannot generate debug world from an empty list of blockstates".into(),
            ));
        }

        let mut storage = Self::new();
        let grid_width = (total as f32).sqrt().ceil() as i32;
        let grid_height = ((total as f32) / (grid_width as f32)).ceil() as i32;

        let max_x = (2 * grid_height + 1).max(1);
        let max_z = (2 * grid_width + 1).max(1);

        // Bounds encompass the generated blocks with boundary safety padding
        storage.set_bounds(0, 69, 0, max_x + 1, 3, max_z + 1);

        for (index, state_ref) in states.iter().enumerate() {
            let state_str = state_ref.as_ref();
            if is_air_state(state_str) {
                continue;
            }

            let i = (index as i32) / grid_width;
            let j = (index as i32) % grid_width;
            let x = 2 * i + 1;
            let y = DEBUG_WORLD_Y;
            let z = 2 * j + 1;

            storage.set_block(x, y, z, state_str, None);
        }

        storage.mark_all_sections_dirty();
        Ok(storage)
    }

    /// Pure-code generation of a canonical Minecraft debug world from a `ResourcePackStack`.
    ///
    /// Scans all blockstate JSONs discovered across all active packs (including vanilla and any
    /// loaded mods), enumerates all possible blockstate combinations, sorts them deterministically,
    /// and lays them out into the canonical debug world grid.
    ///
    /// # Gate
    /// Strictly rejects execution if the pack stack does not contain a valid Minecraft Vanilla Client base pack.
    pub fn create_debug_world_from_pack_stack(
        stack: &mtk_resource::ResourcePackStack,
    ) -> Result<Self, VoxelError> {
        #[cfg(feature = "serde")]
        {
            if stack.is_empty() || !stack.has_vanilla_client_base() {
                return Err(VoxelError::MissingVanillaBasePack(
                    "Cannot generate debug world: active resource pack stack must contain a valid Minecraft Vanilla Client base pack".into(),
                ));
            }

            let locs = stack.list_all_blockstate_locations();
            let mut all_states: Vec<String> = Vec::new();
            let mut seen = HashSet::new();

            for loc in &locs {
                let path = loc.to_asset_path("blockstates", "json");
                if let Some(bytes) = stack.open_asset_raw(&path) {
                    if let Ok(def) =
                        serde_json::from_slice::<mtk_model::BlockStateDefinition>(&bytes)
                    {
                        let states = def.enumerate_all_states(&loc.as_string());
                        for st in states {
                            if seen.insert(st.clone()) {
                                all_states.push(st);
                            }
                        }
                    }
                }
            }

            if all_states.is_empty() {
                return Err(VoxelError::MissingAssets(
                    "Cannot generate debug world: no blockstates discovered across resource pack stack"
                        .into(),
                ));
            }

            all_states.sort();
            Self::create_debug_world_from_states(&all_states)
        }
        #[cfg(not(feature = "serde"))]
        {
            let _ = stack;
            Err(VoxelError::MissingAssets(
                "Cannot generate debug world from pack stack without the `serde` feature in mtk-voxel"
                    .into(),
            ))
        }
    }

    /// Pure-code generation of a canonical Minecraft debug world from a `BakedModelDatabase`.
    ///
    /// Takes all pre-baked blockstate keys already present in the database, sorts them
    /// deterministically, and arranges them in the canonical debug world grid.
    ///
    /// # Gate
    /// Strictly rejects execution if the model database is empty.
    pub fn create_debug_world_from_model_db(
        db: &mtk_model::BakedModelDatabase,
    ) -> Result<Self, VoxelError> {
        if db.is_empty() {
            return Err(VoxelError::MissingAssets(
                "Cannot generate debug world: baked model database is empty".into(),
            ));
        }
        let mut states: Vec<String> = db.keys().cloned().collect();
        states.sort();
        Self::create_debug_world_from_states(&states)
    }

    /// Pure-code generation of a canonical Minecraft debug world from an unpack/resource directory.
    pub fn create_debug_world_from_dir(path: impl AsRef<Path>) -> Result<Self, VoxelError> {
        let p = path.as_ref();
        if !p.exists() || !p.is_dir() {
            return Err(VoxelError::MalformedSnapshot(format!(
                "Directory does not exist or is not a directory: {}",
                p.display()
            )));
        }
        let mut stack = mtk_resource::ResourcePackStack::new();
        let pack = mtk_resource::DirectoryPack::new("DirectoryPack", p);
        stack.append_pack(Box::new(pack));
        Self::create_debug_world_from_pack_stack(&stack)
    }

    /// Default entry point: loads or generates the canonical Minecraft debug world.
    ///
    /// Checks in order:
    /// 1. Memoized in-memory cache;
    /// 2. `MC_ASSETS_DIR` / `MC_DIR` host environment overrides (unpacked vanilla assets).
    ///
    /// Results are memoized for sub-millisecond cloning on repeat requests. Hosts that
    /// reload resource packs at runtime can invalidate the cache via
    /// [`VoxelStorage::clear_debug_world_cache`].
    ///
    /// # Gate
    /// Rejects generation if no valid vanilla Minecraft client base pack is found.
    pub fn create_debug_world() -> Result<Self, VoxelError> {
        if let Ok(guard) = CACHED_DEBUG_STORAGE.read() {
            if let Some(cached) = guard.as_ref() {
                return Ok(cached.clone());
            }
        }

        // 1. Host-level asset root override (CI / custom unpack locations)
        for env_key in &["MC_ASSETS_DIR", "MC_DIR"] {
            if let Ok(env_path) = std::env::var(env_key) {
                let p = Path::new(&env_path);
                if p.join("assets").exists() {
                    if let Ok(storage) = Self::create_debug_world_from_dir(p) {
                        Self::store_debug_world(storage.clone());
                        return Ok(storage);
                    }
                }
            }
        }

        // Rejection gate: no vanilla assets available
        Err(VoxelError::MissingVanillaBasePack(
            "Cannot generate debug world: no vanilla Minecraft client base pack available in environment or cache".into(),
        ))
    }

    fn store_debug_world(storage: VoxelStorage) {
        if let Ok(mut guard) = CACHED_DEBUG_STORAGE.write() {
            *guard = Some(storage);
        }
    }

    /// Clears the internally cached debug storage (useful during live pack reload or tests).
    pub fn clear_debug_world_cache() {
        if let Ok(mut guard) = CACHED_DEBUG_STORAGE.write() {
            *guard = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_code_debug_world_from_states() {
        let sample_states = vec![
            "minecraft:air".to_string(),
            "minecraft:stone".to_string(),
            "minecraft:granite".to_string(),
            "minecraft:oak_stairs[facing=north,half=bottom,shape=straight]".to_string(),
            "minecraft:oak_stairs[facing=south,half=bottom,shape=straight]".to_string(),
            "minecraft:oak_slab[type=bottom]".to_string(),
        ];

        let storage = VoxelStorage::create_debug_world_from_states(&sample_states)
            .expect("Should generate debug world from states");

        let (_min_x, min_y, _min_z, size_x, size_y, size_z) = storage.get_bounds();
        assert_eq!(min_y, 69);
        assert_eq!(size_y, 3);
        assert!(size_x > 0);
        assert!(size_z > 0);

        // Verify air is not placed (returns minecraft:air), non-air is placed at y=70 with odd coordinates
        assert_eq!(storage.get_block(1, 70, 1), "minecraft:air"); // index 0 was air
        assert_eq!(storage.get_block(1, 70, 3), "minecraft:stone");
    }

    #[test]
    fn test_canonical_debug_world_generation() {
        let Some(root) = mtk_testkit::real_assets_root() else {
            eprintln!(
                "[mtk-testkit] skipping canonical debug world test: full vanilla assets unavailable"
            );
            return;
        };

        let storage = VoxelStorage::create_debug_world_from_dir(&root)
            .expect("Should generate debug world from full assets");
        let (min_x, min_y, min_z, size_x, size_y, size_z) = storage.get_bounds();
        assert_eq!(min_x, 0);
        assert_eq!(min_y, 69);
        assert_eq!(min_z, 0);
        assert!(size_x > 100);
        assert_eq!(size_y, 3);
        assert!(size_z > 100);
        assert!(storage.get_all_non_empty_sections().len() > 50);
    }

    #[test]
    fn test_debug_world_gates_reject_missing_assets() {
        // 1. Empty states slice is strictly rejected
        let empty_states: Vec<String> = Vec::new();
        let res_states = VoxelStorage::create_debug_world_from_states(&empty_states);
        assert!(matches!(res_states, Err(VoxelError::MissingAssets(_))));

        // 2. Empty or non-vanilla pack stack is strictly rejected
        let empty_stack = mtk_resource::ResourcePackStack::new();
        let res_stack = VoxelStorage::create_debug_world_from_pack_stack(&empty_stack);
        assert!(matches!(
            res_stack,
            Err(VoxelError::MissingVanillaBasePack(_))
        ));

        // 3. Empty baked model database is strictly rejected
        let empty_db = mtk_model::BakedModelDatabase::new();
        let res_db = VoxelStorage::create_debug_world_from_model_db(&empty_db);
        assert!(matches!(res_db, Err(VoxelError::MissingAssets(_))));
    }

    #[test]
    fn test_debug_world_clone_is_cached_and_fast() {
        VoxelStorage::clear_debug_world_cache();
        let first = match VoxelStorage::create_debug_world() {
            Ok(s) => s,
            Err(_) => {
                // If environment assets are unavailable, synthesize a valid cached storage from sample states
                let sample_states = vec![
                    "minecraft:stone".to_string(),
                    "minecraft:granite".to_string(),
                ];
                let s = VoxelStorage::create_debug_world_from_states(&sample_states).unwrap();
                VoxelStorage::store_debug_world(s.clone());
                s
            }
        };

        // Test repeat cloning speed
        let start = std::time::Instant::now();
        let cloned = VoxelStorage::create_debug_world().expect("Should return cached clone");
        let elapsed = start.elapsed();

        assert_eq!(
            cloned.get_all_non_empty_sections().len(),
            first.get_all_non_empty_sections().len()
        );
        assert!(elapsed.as_millis() < 50, "Cached clone should take <50ms");
    }
}
