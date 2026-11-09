//! Minecraft versions: how they are compared, and which ones DarkClient
//! supports.
//!
//! Shared by the injector (to refuse up front), the client (the authoritative
//! check, from inside the game) and the tooling. Kept dependency-free so it
//! never costs build time to the crates that use it.

use std::fmt;

/// Version ids — exactly as `version.json` / `WorldVersion.id()` report them —
/// DarkClient is verified to work on, with vanilla, Fabric and Forge/NeoForge.
/// Anything else, snapshots included, is refused unless the user forces it.
pub const SUPPORTED_VERSIONS: &[&str] = &["26.1", "26.1.1", "26.1.2", "26.2", "26.3"];

/// Whether `id` is a supported version.
pub fn is_supported(id: &str) -> bool {
    SUPPORTED_VERSIONS.contains(&id)
}

/// An ordered release version, used to gate behaviour that changed between
/// releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MinecraftVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl MinecraftVersion {
    /// Sentinel "newest possible" version, for a build whose version could
    /// not be read: every `version >= X` gate resolves to the latest branch.
    pub const LATEST: MinecraftVersion = MinecraftVersion {
        major: u32::MAX,
        minor: 0,
        patch: 0,
    };

    pub const fn new(major: u32, minor: u32, patch: u32) -> MinecraftVersion {
        MinecraftVersion {
            major,
            minor,
            patch,
        }
    }

    /// Parses a version id: `26.3`, `26.1.2`, or a pre-release such as
    /// `26.4-snapshot-7` / `26.1-rc-1`, which already carries the names of the
    /// release it precedes. Legacy weekly snapshot ids (`26w14a`) have no
    /// ordered form and yield `None`.
    pub fn parse_id(id: &str) -> Option<MinecraftVersion> {
        let release = id.split('-').next()?;
        let mut parts = release.split('.').map(|part| part.parse::<u32>());
        let major = parts.next()?.ok()?;
        let minor = parts.next()?.ok()?;
        let patch = match parts.next() {
            Some(patch) => patch.ok()?,
            None => 0,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(MinecraftVersion::new(major, minor, patch))
    }
}

impl fmt::Display for MinecraftVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_ids_parse_with_an_optional_patch() {
        assert_eq!(
            MinecraftVersion::parse_id("26.3"),
            Some(MinecraftVersion::new(26, 3, 0))
        );
        assert_eq!(
            MinecraftVersion::parse_id("26.1.2"),
            Some(MinecraftVersion::new(26, 1, 2))
        );
    }

    #[test]
    fn pre_releases_parse_as_the_release_they_precede() {
        assert_eq!(
            MinecraftVersion::parse_id("26.4-snapshot-7"),
            Some(MinecraftVersion::new(26, 4, 0))
        );
        assert_eq!(
            MinecraftVersion::parse_id("26.1-rc-1"),
            Some(MinecraftVersion::new(26, 1, 0))
        );
    }

    #[test]
    fn unordered_ids_do_not_parse() {
        assert_eq!(MinecraftVersion::parse_id("26w14a"), None);
        assert_eq!(MinecraftVersion::parse_id("26"), None);
        assert_eq!(MinecraftVersion::parse_id("1.2.3.4"), None);
        assert_eq!(MinecraftVersion::parse_id(""), None);
    }

    #[test]
    fn versions_order_by_release() {
        assert!(MinecraftVersion::new(26, 2, 0) > MinecraftVersion::new(26, 1, 2));
        assert!(MinecraftVersion::LATEST > MinecraftVersion::new(26, 3, 0));
    }

    #[test]
    fn only_listed_ids_are_supported() {
        assert!(is_supported("26.3"));
        assert!(is_supported("26.1.2"));
        assert!(!is_supported("26.4-snapshot-7"));
        assert!(!is_supported("1.21.11"));
        assert!(!is_supported(""));
    }
}
