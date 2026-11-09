//! The binding spec: which game classes and members the client uses, as
//! named in the newest supported version. One TOML file per area under
//! `mc-data/spec/`.
//!
//! ```toml
//! [[class]]
//! name = "net/minecraft/client/renderer/GameRenderer"
//!
//!   [[class.method]]
//!   name = "mainCamera"            # descriptor inferred when not overloaded
//!
//!   [[class.field]]
//!   name = "gameRenderState"
//! ```

use std::path::Path;

use serde::Deserialize;

use crate::Error;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecFile {
    #[serde(default, rename = "class")]
    pub classes: Vec<ClassSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassSpec {
    /// JNI name in the newest supported version.
    pub name: String,
    /// Rust type name; defaults to the simple class name.
    pub rust: Option<String>,
    /// Not part of the game jar (JDK, libraries): not checked against the
    /// dumps, members bound exactly as written.
    #[serde(default)]
    pub external: bool,
    /// First version the class exists in.
    pub since: Option<String>,
    #[serde(default, rename = "method")]
    pub methods: Vec<MemberSpec>,
    #[serde(default, rename = "field")]
    pub fields: Vec<MemberSpec>,
    #[serde(default, rename = "constructor")]
    pub constructors: Vec<MemberSpec>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSpec {
    /// Java name; not used for constructors.
    pub name: Option<String>,
    /// JNI descriptor. Required for overloaded names, constructors, external
    /// classes, and members whose newest version is provided by a shim.
    pub descriptor: Option<String>,
    /// Rust function name; defaults to the Java name in snake case (`new`
    /// for a constructor).
    pub rust: Option<String>,
    /// First version the member exists in.
    pub since: Option<String>,
    /// First version the member no longer exists in.
    pub until: Option<String>,
    /// Fields: also generate a setter.
    #[serde(default)]
    pub set: bool,
    /// External classes only: the member is static.
    #[serde(default, rename = "static")]
    pub is_static: bool,
}

/// Reads every `*.toml` under `dir`, in file-name order.
pub fn load(dir: &Path) -> Result<Vec<(String, SpecFile)>, Error> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| Error::io(dir, e))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            let spec = toml::from_str(&text)
                .map_err(|e| Error::Spec(format!("{}: {e}", path.display())))?;
            Ok((path.display().to_string(), spec))
        })
        .collect()
}
