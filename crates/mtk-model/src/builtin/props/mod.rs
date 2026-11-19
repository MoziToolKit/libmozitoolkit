//! Procedural geometry for block kinds whose mesh is a pure function of the blockstate.
//!
//! These generators stay in the core because such a block is still a block: it is placed on the
//! voxel grid, selected, and streamed exactly like any other block. Continuous entities (which
//! exist off-grid and would need their own instance stream) are a separate, opt-in pack.

pub(crate) mod chest;
pub(crate) mod copper_statue;
pub(crate) mod cube;
pub(crate) mod shulker;
