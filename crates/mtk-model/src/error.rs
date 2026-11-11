use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ModelError {
    #[error("Invalid BlockState syntax: '{0}'")]
    InvalidBlockStateSyntax(String),

    #[error("JSON deserialization error: {0}")]
    JsonError(String),

    #[error("Model parent hierarchy cycle detected: '{0}'")]
    CircularParentHierarchy(String),

    #[error("Texture variable unresolved: '#{0}'")]
    UnresolvedTextureVariable(String),

    #[error("Missing expected model element face for direction: '{0}'")]
    MissingFace(String),

    #[error("ThreadPool error: {0}")]
    ThreadPoolError(String),

    #[error("Postcard serialization error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("Entity validation error: {0}")]
    EntityValidation(#[from] crate::entity::EntityValidationError),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Invalid model database payload: {0}")]
    InvalidDatabasePayload(String),
}
