//! Typed bindings to the Minecraft members DarkClient uses, on every
//! supported version.
//!
//! `generated.rs` is written by `cargo xtask bindgen` from `mc-data/`: the
//! spec names each member as the newest version does, the adapters describe
//! how older versions differ, and the dumps check that every member resolves
//! on every version. Each class is a unit struct whose associated functions
//! take a [`Cx`] and the receiver: `GameRenderer::main_camera(cx, &renderer)`
//! calls `mainCamera()` on 26.2+ and `getMainCamera()` before. Members a
//! version lacks return [`Error::Unavailable`] there.
//!
//! Object results are JNI local references: run loops inside a local frame.

mod generated;
mod shims;

pub use generated::*;
pub use mc_runtime::{Cx, Error, MinecraftVersion, Result, Runtime};

use generated::VERSIONS;

/// The binding group of the running version, chosen once per session.
fn group(cx: &Cx) -> usize {
    cx.runtime()
        .binding_group(|runtime| group_of(runtime.version_id(), runtime.version()))
}

/// A supported version's group; any other version (forced by the user) uses
/// the newest supported version not newer than it, or the oldest one.
fn group_of(id: Option<&str>, version: MinecraftVersion) -> usize {
    if let Some((_, _, group)) = VERSIONS.iter().find(|(known, ..)| Some(*known) == id) {
        return *group;
    }
    VERSIONS
        .iter()
        .rev()
        .find(|(_, known, _)| *known <= version)
        .or(VERSIONS.first())
        .map_or(0, |(_, _, group)| *group)
}

fn unavailable(cx: &Cx, member: &'static str) -> Error {
    Error::Unavailable {
        member,
        version: cx.runtime().version_id().unwrap_or("unknown").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newest() -> usize {
        VERSIONS.last().unwrap().2
    }

    #[test]
    fn supported_versions_use_their_own_group() {
        for (id, version, group) in VERSIONS {
            assert_eq!(group_of(Some(id), *version), *group);
        }
    }

    #[test]
    fn other_versions_fall_back_to_the_nearest_older_one() {
        let (_, oldest, oldest_group) = VERSIONS[0];
        assert_eq!(
            group_of(Some("27.1-snapshot-1"), MinecraftVersion::new(27, 1, 0)),
            newest()
        );
        assert_eq!(group_of(None, MinecraftVersion::LATEST), newest());
        assert_eq!(
            group_of(Some("1.21.11"), MinecraftVersion::new(1, 21, 11)),
            oldest_group
        );
        assert_eq!(group_of(Some("odd"), oldest), oldest_group);
    }
}
