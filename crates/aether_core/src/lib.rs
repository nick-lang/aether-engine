//! Core identifiers, versioning, and shared errors.

use std::fmt;

pub const ENGINE_NAME: &str = "Aether Engine";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageId(pub String);

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageVersion(pub u64);

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("package not found: {0}")]
    PackageNotFound(String),
    #[error("invalid package: {0}")]
    InvalidPackage(String),
    #[error("patch rejected: {0}")]
    PatchRejected(String),
}

pub type EngineResult<T> = Result<T, EngineError>;
