//! Error types aligned with Python litemapy.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("NBT binary error: {0}")]
    Nbt(#[from] valence_nbt::binary::Error),

    #[error("unsupported Litematica / NBT field: {0}")]
    Corrupted(String),

    #[error("{0}")]
    Value(String),

    #[error("missing required NBT field: {context}.{key}")]
    MissingField { context: &'static str, key: String },

    #[error("unexpected NBT tag in {context}: expected {expected}, got tag {got:?}")]
    UnexpectedTag {
        context: &'static str,
        expected: String,
        got: valence_nbt::Tag,
    },
}

impl From<std::fmt::Error> for Error {
    fn from(_: std::fmt::Error) -> Self {
        Error::Corrupted("format".into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[error("{0:?}")]
pub struct CorruptedSchematicError(pub String);

#[derive(Debug, Error)]
#[error("invalid identifier \"{identifier}\"")]
pub struct InvalidIdentifier {
    pub identifier: String,
}

#[derive(Debug, Error)]
#[error("{key} -> {message}")]
pub struct RequiredKeyMissingException {
    pub key: String,
    pub message: String,
}

impl RequiredKeyMissingException {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: "The required key is missing in the (Tile)Entity's NBT Compound".into(),
        }
    }
}

#[derive(Debug, Error)]
#[error("discrimination error: {0}")]
pub struct DiscriminationError(pub String);
