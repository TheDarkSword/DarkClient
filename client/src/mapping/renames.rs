//! Registry of Mojmap symbols — classes, methods, fields — that Mojang renamed
//! across Minecraft versions.
//!
//! The rest of the codebase always uses a symbol's **latest** name (a class via
//! its [`MinecraftClassType`], a method/field as a string literal). This table
//! translates that name back for older builds, so one build keeps working on
//! every supported Minecraft version.
//!
//! See `docs_internal/versioned-names.md` for the design rationale.

use crate::mapping::minecraft_version::MinecraftVersion;
use crate::mapping::MinecraftClassType;

/// Shorthand for a [`MinecraftVersion`] literal in the [`RENAMES`] table.
const fn v(major: u32, minor: u32, patch: u32) -> MinecraftVersion {
    MinecraftVersion::new(major, minor, patch)
}

/// A Mojmap symbol Mojang renamed over time.
///
/// `legacy` lists the older names **oldest first**: each `(replaced_in, name)`
/// means `name` was used up to — but not including — version `replaced_in`.
enum Rename {
    /// A renamed class. Its current name is taken from
    /// [`MinecraftClassType::get_name`], so it is never repeated here — rename
    /// the class in one place and this entry follows.
    Class {
        class: MinecraftClassType,
        legacy: &'static [(MinecraftVersion, &'static str)],
    },
    /// A renamed method or field of `owner`. `canonical` is its current name.
    Member {
        owner: MinecraftClassType,
        canonical: &'static str,
        legacy: &'static [(MinecraftVersion, &'static str)],
    },
}

/// Every symbol whose name changed across the supported Minecraft versions.
static RENAMES: &[Rename] = &[
    // `Window.getWindow()` -> `Window.handle()`
    Rename::Member {
        owner: MinecraftClassType::Window,
        canonical: "handle",
        legacy: &[(v(1, 21, 9), "getWindow")],
    },
    // `MultiPlayerGameMode.handleInventoryMouseClick()` -> `handleContainerInput()`
    Rename::Member {
        owner: MinecraftClassType::MultiPlayerGameMode,
        canonical: "handleContainerInput",
        legacy: &[(v(26, 1, 0), "handleInventoryMouseClick")],
    },
    // `GameRenderer.getMainCamera()` -> `GameRenderer.mainCamera()`
    Rename::Member {
        owner: MinecraftClassType::GameRenderer,
        canonical: "mainCamera",
        legacy: &[(v(26, 2, 0), "getMainCamera")],
    },
    // class `ClickType` -> class `ContainerInput`
    Rename::Class {
        class: MinecraftClassType::ContainerInput,
        legacy: &[(v(26, 1, 0), "net/minecraft/world/inventory/ClickType")],
    },
];

/// Walks `legacy` (oldest first) and returns the first older name whose range
/// contains `version`, or `current` if none do.
fn pick<'a>(
    current: &'a str,
    legacy: &[(MinecraftVersion, &'static str)],
    version: MinecraftVersion,
) -> &'a str {
    for &(replaced_in, old_name) in legacy {
        if version < replaced_in {
            return old_name;
        }
    }
    current
}

/// The JNI name of the class `name` on Minecraft `version` — `name` itself
/// unless an older name applies.
pub fn resolve_class(name: &str, version: MinecraftVersion) -> &str {
    // Reflected builds always run the latest Minecraft: nothing to translate.
    if version == MinecraftVersion::LATEST {
        return name;
    }
    for rename in RENAMES {
        if let Rename::Class { class, legacy } = rename {
            if class.get_name() == name {
                return pick(name, legacy, version);
            }
        }
    }
    name
}

/// The name of method/field `canonical` of class `owner` on Minecraft
/// `version` — `canonical` itself unless an older name applies.
pub fn resolve_member(
    owner: MinecraftClassType,
    canonical: &str,
    version: MinecraftVersion,
) -> &str {
    if version == MinecraftVersion::LATEST {
        return canonical;
    }
    for rename in RENAMES {
        if let Rename::Member {
            owner: entry_owner,
            canonical: entry_canonical,
            legacy,
        } = rename
        {
            if *entry_owner == owner && *entry_canonical == canonical {
                return pick(canonical, legacy, version);
            }
        }
    }
    canonical
}
