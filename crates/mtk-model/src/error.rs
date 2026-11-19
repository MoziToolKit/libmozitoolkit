use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ModelError {
    #[error("Invalid BlockState syntax: '{0}'")]
    InvalidBlockStateSyntax(String),

    #[error("JSON deserialization error: {0}")]
    JsonError(String),

    #[error("Model parent hierarchy cycle detected: '{0}'")]
    CircularParentHierarchy(String),

    /// A texture variable resolves to itself.
    ///
    /// Templates do this on purpose (`"particle": "#particle"`) and rely on a child to supply the
    /// value, so this is not a defect in the model that defines it; callers must not report it as a
    /// broken hierarchy.
    #[error("Cyclic texture variable '#{0}'")]
    CyclicTextureVariable(String),

    #[error("Texture variable unresolved: '#{0}'")]
    UnresolvedTextureVariable(String),

    #[error("Missing expected model element face for direction: '{0}'")]
    MissingFace(String),

    #[error("ThreadPool error: {0}")]
    ThreadPoolError(String),

    #[error("Postcard serialization error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Invalid model database payload: {0}")]
    InvalidDatabasePayload(String),

    #[error("Item mesh baking error: {0}")]
    ItemBakeError(String),
}
