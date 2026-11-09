//! Tooling behind DarkClient's version support: per-version dumps of the
//! Minecraft classes the client binds, read straight from the game jars, and
//! the bindings generated from them.
//!
//! - [`fetch`] downloads (and caches) a version's client jar from Mojang;
//! - [`classfile`] reads the bound classes and their supertypes out of a jar;
//! - [`dump`] is the committed, diff-friendly text form of the result;
//! - [`spec`] lists the members the client uses, [`adapters`] what changed
//!   between versions, [`resolve`] binds the one to every dump through the
//!   other, and [`codegen`] writes the result out as Rust.

pub mod adapters;
pub mod classfile;
pub mod codegen;
pub mod dump;
pub mod fetch;
pub mod resolve;
pub mod spec;

use std::path::Path;

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
    #[error("{0}")]
    Spec(String),
}

impl Error {
    pub(crate) fn io(path: impl AsRef<std::path::Path>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }
}

/// The generated sources, unformatted.
pub struct Generated {
    /// `mc-bindings/src/generated.rs`.
    pub bindings: String,
    /// `mc-versions/src/supported.rs`.
    pub supported: String,
}

/// Reads `mc-data/` (`spec/`, `adapters/`, `dumps/`) and generates the
/// bindings. Every resolution problem is reported at once, one per line.
pub fn generate(data: &Path) -> Result<Generated, Error> {
    let specs = spec::load(&data.join("spec"))?;
    let adapters = adapters::load(&data.join("adapters"))?;
    let dumps_dir = data.join("dumps");
    let mut dumps = Vec::new();
    let entries = std::fs::read_dir(&dumps_dir).map_err(|e| Error::io(&dumps_dir, e))?;
    for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
        if path.extension().is_some_and(|ext| ext == "txt") {
            let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            dumps.push(dump::Dump::parse(&text)?);
        }
    }
    let model = resolve::resolve(&specs, &dumps, &adapters)
        .map_err(|errors| Error::Spec(errors.join("\n")))?;
    Ok(Generated {
        bindings: codegen::bindings(&model).map_err(Error::Spec)?,
        supported: codegen::supported(&model),
    })
}
