use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MinecraftVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl MinecraftVersion {
    /// Sentinel "newest possible" version, used for unobfuscated builds where
    /// no version string is parsed: it makes every `version < X` gate resolve
    /// to the latest branch.
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

    /// Parses a game version id as `WorldVersion.id()` reports it: `26.3`,
    /// `26.1.2`, or a pre-release such as `26.4-snapshot-7` / `26.1-rc-1`,
    /// which already carries the names of the release it precedes. Legacy
    /// weekly snapshot ids (`26w14a`) have no ordered form and yield `None`.
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

impl std::fmt::Display for MinecraftVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl<'de> Deserialize<'de> for MinecraftVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split('.').collect();

        if parts.len() != 3 {
            return Err(serde::de::Error::custom(format!(
                "Invalid version format: {} (expected major.minor.patch)",
                s
            )));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom("Invalid major version"))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom("Invalid minor version"))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom("Invalid patch version"))?;

        Ok(MinecraftVersion {
            major,
            minor,
            patch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::MinecraftVersion;

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
        assert_eq!(
            MinecraftVersion::parse_id("1.21.11"),
            Some(MinecraftVersion::new(1, 21, 11))
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
}
