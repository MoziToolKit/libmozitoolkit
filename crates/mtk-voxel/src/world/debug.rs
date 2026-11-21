//! Debug world factory methods for VoxelWorld.

use std::sync::Arc;

use mtk_cull::FaceCuller;
use mtk_model::BakedModelDatabase;

use crate::storage::VoxelStorage;
use crate::types::{MesherConfig, VoxelError};

use super::VoxelWorld;

impl VoxelWorld {
    /// Loads the debug world from a pre-baked model database.
    ///
    /// There is no environment-driven entry point: the caller decides which assets to use (the
    /// host resolves its asset root and passes the database or a pack stack).
    pub fn create_debug_world_from_model_db(
        model_db: &Arc<BakedModelDatabase>,
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        num_threads: Option<usize>,
    ) -> Result<Self, VoxelError> {
        let storage = VoxelStorage::create_debug_world_from_model_db(model_db)?;
        let mut world = Self::from_storage(storage, config, culler, Some(Arc::clone(model_db)));
        world.num_threads = num_threads;
        Ok(world)
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
        let mut world = Self::from_storage(storage, config, culler, model_db);
        world.num_threads = num_threads;
        Ok(world)
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
        let mut world = Self::from_storage(storage, config, culler, model_db);
        world.num_threads = num_threads;
        Ok(world)
    }
}
