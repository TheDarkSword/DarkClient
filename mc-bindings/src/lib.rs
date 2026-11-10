//! The Minecraft classes as Rust types, for every supported version.
//! `generated.rs` comes from `cargo xtask bindgen`; never edit it by hand.

mod generated;
mod shims;

pub use generated::*;
pub use mc_runtime::{
    runtime, BindingReport, Error, JavaClass, JavaType, MinecraftVersion, Object, Result, Runtime,
};

use generated::{CHECKS, VERSIONS};
use mc_runtime::Cx;

fn cx() -> Result<Cx<'static>> {
    Cx::current()
}

fn group() -> Result<usize> {
    Ok(runtime()?.binding_group(|runtime| group_of(runtime.version_id(), runtime.version())))
}

/// A forced, unlisted version uses the newest supported one not newer than it.
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

/// Checks that every class and member the running version binds exists in the
/// game, without initializing any class.
pub fn verify() -> Result<BindingReport> {
    let group = group()?;
    runtime()?.verify(
        CHECKS
            .iter()
            .filter(|(groups, _)| groups & (1 << group) != 0)
            .map(|(_, check)| check),
    )
}

fn unavailable(member: &'static str) -> Error {
    let version = runtime()
        .ok()
        .and_then(|runtime| runtime.version_id())
        .unwrap_or("unknown")
        .to_string();
    Error::Unavailable { member, version }
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
