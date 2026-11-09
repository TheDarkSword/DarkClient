//! Tooling behind DarkClient's version support: per-version dumps of the
//! Minecraft classes the client binds, read straight from the game jars.
//!
//! - [`fetch`] downloads (and caches) a version's client jar from Mojang;
//! - [`classfile`] reads the bound classes and their supertypes out of a jar;
//! - [`dump`] is the committed, diff-friendly text form of the result.

pub mod classfile;
pub mod dump;
pub mod fetch;

use thiserror::Error;

/// Anything that can go wrong producing or reading a dump.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0}: not a valid zip archive: {1}")]
    Zip(String, zip::result::ZipError),
    #[error("{class}: unreadable class file: {message}")]
    ClassFile { class: String, message: String },
    #[error("dump line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("{0}")]
    Fetch(String),
}

impl Error {
    pub(crate) fn io(path: impl AsRef<std::path::Path>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }
}
