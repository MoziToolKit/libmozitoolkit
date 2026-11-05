//! Debug world factory methods for VoxelWorld.

use std::sync::Arc;

use mtk_cull::FaceCuller;
use mtk_model::BakedModelDatabase;

use crate::storage::VoxelStorage;
use crate::types::{MesherConfig, VoxelError};

use super::VoxelWorld;

impl VoxelWorld {
    /// Loads the canonical embedded Minecraft debug world into a new `VoxelWorld`.
    pub fn create_debug_world(
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
    ) -> Result<Self, VoxelError> {
        Self::create_debug_world_with_threads(config, culler, model_db, None)
    }

    /// Loads the canonical embedded Minecraft debug world into a new `VoxelWorld` with explicit thread configuration.
    pub fn create_debug_world_with_threads(
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
        num_threads: Option<usize>,
    ) -> Result<Self, VoxelError> {
        let storage = if let Some(ref db) = model_db {
            if !db.is_empty() {
                VoxelStorage::create_debug_world_from_model_db(db)?
            } else {
                return Err(VoxelError::MissingAssets(
                    "Cannot generate debug world: baked model database is empty".into(),
                ));
            }
        } else {
            VoxelStorage::create_debug_world()?
        };
        Ok(Self::from_storage_with_threads(
            storage,
            config,
            culler,
            model_db,
            num_threads,
        ))
    }

    /// Creates a debug `VoxelWorld` from an explicit list of blockstate strings.
    pub fn create_debug_world_from_states<S: AsRef<str>>(
        states: &[S],
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
        num_threads: Option<usize>,
    ) -> Result<Self, VoxelError> {
        let storage = VoxelStorage::create_debug_world_from_states(states)?;
        Ok(Self::from_storage_with_threads(
            storage,
            config,
            culler,
            model_db,
            num_threads,
        ))
    }

    /// Creates a debug `VoxelWorld` by enumerating blockstates across a `ResourcePackStack`.
    pub fn create_debug_world_from_pack_stack(
        stack: &mtk_resource::ResourcePackStack,
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
        num_threads: Option<usize>,
    ) -> Result<Self, VoxelError> {
        let storage = VoxelStorage::create_debug_world_from_pack_stack(stack)?;
        Ok(Self::from_storage_with_threads(
            storage,
            config,
            culler,
            model_db,
            num_threads,
        ))
    }
}
