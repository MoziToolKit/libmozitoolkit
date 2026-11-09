//! Mesh Attribute Schema and Data Definitions.

pub mod schema;
mod types;

pub use schema::{constants, MeshAttributeSpec, MESH_ATTRIBUTE_SPECS, RETIRED_MESH_ATTRIBUTES};
pub use types::{
    AttributeData, AttributeDomain, FaceAttributes, MaterialSlotId, MeshAttribute, TintIndex,
};

#[cfg(test)]
mod tests;
