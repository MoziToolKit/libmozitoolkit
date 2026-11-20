//! Parallel and batch baking of Minecraft BlockStates.

use crate::baker::baked_model::BakedModel;
use crate::baker::model_baker::ModelBaker;
use crate::error::ModelError;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;

/// Default thread budget for parallel baking.
///
/// Follows the process-wide policy (`mtk_core::threading::resolved_threads`, i.e. all cores
/// minus two unless the host set an explicit count) instead of the old fixed cap of four, so a
/// host that configures its budget once gets the same answer from every entry point.
pub fn determine_conservative_threads() -> usize {
    mtk_core::threading::resolved_threads()
}

/// Bakes a batch of BlockStates.
///
/// - When `feature = "parallel"` is enabled: parallelizes across worker threads.
/// - When compiled for WASM or single-threaded mode: falls back to sequential iteration.
pub fn bake_batch<S, SF, MF>(
    states: &[S],
    state_loader: SF,
    model_loader: MF,
) -> Result<Vec<(String, Result<BakedModel, ModelError>)>, ModelError>
where
    S: AsRef<str> + Sync,
    SF: Fn(&str) -> Option<BlockStateDefinition> + Sync + Send,
    MF: Fn(&str) -> Option<BlockModelJson> + Sync + Send,
{
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        let results = states
            .par_iter()
            .map(|state_ref| {
                let state_str = state_ref.as_ref();
                let mut local_baker = ModelBaker::new();
                let res = (|| -> Result<BakedModel, ModelError> {
                    let bs = BlockState::parse(state_str)?;
                    let bs_def = state_loader(&bs.name);
                    local_baker.bake_blockstate(state_str, bs_def.as_ref(), |id| model_loader(id))
                })();
                (state_str.to_string(), res)
            })
            .collect();
        Ok(results)
    }

    #[cfg(not(feature = "parallel"))]
    {
        let results = states
            .iter()
            .map(|state_ref| {
                let state_str = state_ref.as_ref();
                let mut local_baker = ModelBaker::new();
                let res = (|| -> Result<BakedModel, ModelError> {
                    let bs = BlockState::parse(state_str)?;
                    let bs_def = state_loader(&bs.name);
                    local_baker.bake_blockstate(state_str, bs_def.as_ref(), |id| model_loader(id))
                })();
                (state_str.to_string(), res)
            })
            .collect();
        Ok(results)
    }
}

/// Backwards-compatible batch baker with optional explicit thread pool configuration.
pub fn bake_batch_parallel<S, SF, MF>(
    states: &[S],
    num_threads: Option<usize>,
    state_loader: SF,
    model_loader: MF,
) -> Result<Vec<(String, Result<BakedModel, ModelError>)>, ModelError>
where
    S: AsRef<str> + Sync,
    SF: Fn(&str) -> Option<BlockStateDefinition> + Sync + Send,
    MF: Fn(&str) -> Option<BlockModelJson> + Sync + Send,
{
    mtk_core::threading::execute_parallel(num_threads, || {
        bake_batch(states, state_loader, model_loader)
    })
    .map_err(ModelError::ThreadPoolError)?
}

impl ModelBaker {
    /// Calculates a conservative default thread count for parallel operations.
    pub fn determine_conservative_threads() -> usize {
        determine_conservative_threads()
    }

    /// Bakes a batch of BlockStates.
    pub fn bake_batch<S, SF, MF>(
        states: &[S],
        state_loader: SF,
        model_loader: MF,
    ) -> Result<Vec<(String, Result<BakedModel, ModelError>)>, ModelError>
    where
        S: AsRef<str> + Sync,
        SF: Fn(&str) -> Option<BlockStateDefinition> + Sync + Send,
        MF: Fn(&str) -> Option<BlockModelJson> + Sync + Send,
    {
        bake_batch(states, state_loader, model_loader)
    }

    /// Backwards-compatible batch baker with optional explicit thread pool configuration.
    pub fn bake_batch_parallel<S, SF, MF>(
        states: &[S],
        num_threads: Option<usize>,
        state_loader: SF,
        model_loader: MF,
    ) -> Result<Vec<(String, Result<BakedModel, ModelError>)>, ModelError>
    where
        S: AsRef<str> + Sync,
        SF: Fn(&str) -> Option<BlockStateDefinition> + Sync + Send,
        MF: Fn(&str) -> Option<BlockModelJson> + Sync + Send,
    {
        bake_batch_parallel(states, num_threads, state_loader, model_loader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "parallel")]
    fn test_bake_batch_parallel() {
        let model_json = r##"{
            "textures": { "all": "minecraft:block/stone" },
            "elements": [{
                "from": [0, 0, 0], "to": [16, 16, 16],
                "faces": {
                    "down":  { "texture": "#all" }, "up":    { "texture": "#all" },
                    "north": { "texture": "#all" }, "south": { "texture": "#all" },
                    "west":  { "texture": "#all" }, "east":  { "texture": "#all" }
                }
            }]
        }"##;
        let model: BlockModelJson = serde_json::from_str(model_json).unwrap();

        let states = vec![
            "minecraft:stone".to_string(),
            "minecraft:stone[variant=smooth]".to_string(),
            "minecraft:stone[variant=rough]".to_string(),
        ];

        let results = ModelBaker::bake_batch_parallel(
            &states,
            Some(2),
            |_name| None,
            |_id| Some(model.clone()),
        )
        .unwrap();

        assert_eq!(results.len(), 3);
        for (st, res) in results {
            let baked = res.unwrap();
            assert!(baked.is_cube);
            assert_eq!(baked.elements.len(), 1);
            assert!(states.contains(&st));
        }
    }
}
