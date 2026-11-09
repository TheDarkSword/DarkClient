//! Version deltas, ViaVersion-style: `mc-data/adapters/<version>.toml` lists
//! what changed in that version relative to the previous supported one, so
//! the generator can walk from the newest version down to any older one.
//!
//! ```toml
//! # 26.2: GameRenderer.getMainCamera() became mainCamera().
//! [[rename]]
//! member = "GameRenderer.mainCamera"
//! before = "getMainCamera"
//!
//! # 26.3: the Vulkan backend moved package.
//! [[class]]
//! name = "com/mojang/renderpearl/backend/vulkan/VulkanDevice"
//! before = "com/mojang/blaze3d/vulkan/VulkanDevice"
//!
//! # 26.3: swing() needs the held item's attack animation.
//! [[shim]]
//! member = "LivingEntity.swing"
//! since = "swing_with_attack_animation"
//! ```
//!
//! Members are named `RustClass.javaName`, with the spec descriptor appended
//! when the name is overloaded: `LivingEntity.swing(Lnet/…/InteractionHand;)V`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::Error;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterFile {
    /// A member renamed in this version: `before` is its name in older ones.
    #[serde(default, rename = "rename")]
    pub renames: Vec<Rename>,
    /// A class renamed (or moved) in this version.
    #[serde(default, rename = "class")]
    pub classes: Vec<ClassRename>,
    /// A member provided by a hand-written function in `mc-bindings/src/shims.rs`.
    #[serde(default, rename = "shim")]
    pub shims: Vec<Shim>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    pub member: String,
    /// Name before this version.
    pub before: String,
    /// Descriptor before this version, when it changed too. Class renames
    /// are applied to descriptors automatically.
    pub descriptor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassRename {
    /// JNI name from this version on.
    pub name: String,
    /// JNI name before this version.
    pub before: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shim {
    pub member: String,
    /// The shim provides the member from this version on.
    pub since: Option<String>,
    /// The shim provides the member before this version.
    pub before: Option<String>,
}

/// Reads every `<version>.toml` under `dir`, keyed by version id.
pub fn load(dir: &Path) -> Result<BTreeMap<String, AdapterFile>, Error> {
    let mut adapters = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(adapters);
    };
    for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
        let (Some(stem), Some("toml")) = (
            path.file_stem().and_then(|stem| stem.to_str()),
            path.extension().and_then(|ext| ext.to_str()),
        ) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let file =
            toml::from_str(&text).map_err(|e| Error::Spec(format!("{}: {e}", path.display())))?;
        adapters.insert(stem.to_string(), file);
    }
    Ok(adapters)
}
