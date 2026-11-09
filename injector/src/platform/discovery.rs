//! Cross-platform discovery of running Minecraft instances.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sysinfo::System;

/// A Minecraft process the user can inject into.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Operating-system process id.
    pub pid: u32,
    /// The game version id (`26.3`), when it could be read off the command
    /// line or the game jar.
    pub version: Option<String>,
    /// Human-readable label: version, mod loader, and whether the version is
    /// supported.
    pub info: String,
}

/// Scans running processes and returns every Java process that looks like a
/// Minecraft client, sorted by pid.
pub fn find_minecraft_processes() -> Vec<ProcessInfo> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut found: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let exe = process.name().to_string_lossy();
            let args: Vec<String> = process
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            if !is_minecraft(&exe, &args) {
                return None;
            }
            let version = detect_version(&args);
            Some(ProcessInfo {
                pid: pid.as_u32(),
                info: label(&args, version.as_deref()),
                version,
            })
        })
        .collect();

    found.sort_by_key(|p| p.pid);
    found
}

/// Whether a process — its executable name and arguments — is a Minecraft
/// client.
fn is_minecraft(exe_name: &str, args: &[String]) -> bool {
    is_java_executable(exe_name) && args.join(" ").to_lowercase().contains("minecraft")
}

/// True if `exe_name` is a Java launcher binary, ignoring case and any
/// platform extension (`java`, `javaw`, `java.exe`, `javaw.exe`).
fn is_java_executable(exe_name: &str) -> bool {
    let stem = Path::new(exe_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_name);
    stem.eq_ignore_ascii_case("java") || stem.eq_ignore_ascii_case("javaw")
}

/// The game version. Forge and NeoForge pass it as `--fml.mcVersion`; vanilla
/// and Fabric keep the client jar — with its `version.json` — on the class
/// path. `--version` is not used: it names the launcher profile
/// (`fabric-loader-0.18.6-26.1.1`, a custom instance name, …).
///
/// Best effort: `None` leaves the decision to the client, which reads the
/// version from inside the game.
fn detect_version(args: &[String]) -> Option<String> {
    if let Some(version) = arg_value(args, "--fml.mcVersion") {
        return Some(version.to_string());
    }
    let mut jars = launch_jars(args);
    // The game jar usually sits under `versions/`: try the likely ones first.
    jars.sort_by_key(|jar| !looks_like_game_jar(jar));
    jars.iter().find_map(|jar| read_version_id(jar))
}

/// The value following `flag`, either as the next argument or as
/// `flag=value`.
fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, arg)| {
        if arg == flag {
            args.get(index + 1).map(String::as_str)
        } else {
            arg.strip_prefix(flag)?.strip_prefix('=')
        }
    })
}

/// Every `.jar` on the class path or the module path.
fn launch_jars(args: &[String]) -> Vec<PathBuf> {
    ["-cp", "-classpath", "--class-path", "-p", "--module-path"]
        .iter()
        .filter_map(|flag| arg_value(args, flag))
        .flat_map(std::env::split_paths)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
        })
        .collect()
}

fn looks_like_game_jar(jar: &Path) -> bool {
    let path = jar.to_string_lossy().to_lowercase();
    ["versions", "minecraft", "client"]
        .iter()
        .any(|hint| path.contains(hint))
}

/// The `id` of the `version.json` inside `jar`, if it is a Minecraft client
/// jar.
fn read_version_id(jar: &Path) -> Option<String> {
    let mut archive = zip::ZipArchive::new(File::open(jar).ok()?).ok()?;
    archive
        .by_name("net/minecraft/client/Minecraft.class")
        .ok()?;
    let mut text = String::new();
    archive
        .by_name("version.json")
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    json_string_field(&text, "id")
}

/// The string value of top-level-looking `"key": "value"` in `json` — enough
/// for `version.json`, without a JSON dependency.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let quoted_key = format!("\"{key}\"");
    let mut rest = json;
    while let Some(at) = rest.find(&quoted_key) {
        rest = &rest[at + quoted_key.len()..];
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        if let Some(value) = value.trim_start().strip_prefix('"') {
            return value.split('"').next().map(str::to_string);
        }
    }
    None
}

/// Which mod loader launched the game, from its launch arguments.
fn mod_loader(args: &[String]) -> &'static str {
    let joined = args.join(" ");
    if joined.contains("--fml.neoForgeVersion") {
        "NeoForge"
    } else if joined.contains("--fml.forgeVersion") || joined.contains("--fml.mcVersion") {
        "Forge"
    } else if joined.contains("net.fabricmc") || joined.contains("fabric-loader") {
        "Fabric"
    } else if joined.contains("org.quiltmc") {
        "Quilt"
    } else {
        "Vanilla"
    }
}

fn label(args: &[String], version: Option<&str>) -> String {
    let loader = mod_loader(args);
    match version {
        Some(version) if mc_versions::is_supported(version) => {
            format!("Minecraft {version} · {loader}")
        }
        Some(version) => format!("Minecraft {version} · {loader} · unsupported"),
        None => format!("Minecraft (unknown version) · {loader}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_plain_java_minecraft_process_is_detected() {
        assert!(is_minecraft(
            "java",
            &args(&["-cp", "minecraft.jar", "net.minecraft.client.main.Main"]),
        ));
    }

    #[test]
    fn the_windows_javaw_exe_is_detected() {
        assert!(is_minecraft(
            "javaw.exe",
            &args(&["-Xmx2G", "minecraft", "--gameDir", "."])
        ));
    }

    #[test]
    fn the_java_binary_match_is_case_insensitive() {
        assert!(is_minecraft("JavaW", &args(&["net.minecraft.client"])));
    }

    #[test]
    fn the_minecraft_keyword_match_is_case_insensitive() {
        assert!(is_minecraft("java", &args(&["-jar", "MINECRAFT.jar"])));
    }

    #[test]
    fn a_non_java_process_is_ignored() {
        assert!(!is_minecraft("python", &args(&["minecraft_server.py"])));
    }

    #[test]
    fn a_java_process_without_minecraft_is_ignored() {
        assert!(!is_minecraft("java", &args(&["-jar", "build-tools.jar"])));
    }

    #[test]
    fn forge_and_neoforge_report_the_game_version_on_the_command_line() {
        let launch = args(&["--launchTarget", "forgeclient", "--fml.mcVersion", "26.1.2"]);
        assert_eq!(detect_version(&launch), Some("26.1.2".to_string()));
    }

    #[test]
    fn flag_values_are_read_in_both_forms() {
        let launch = args(&["-cp", "a.jar", "--module-path=b.jar"]);
        assert_eq!(arg_value(&launch, "-cp"), Some("a.jar"));
        assert_eq!(arg_value(&launch, "--module-path"), Some("b.jar"));
        assert_eq!(arg_value(&launch, "-p"), None);
    }

    #[test]
    fn the_version_comes_from_the_game_jar_on_the_class_path() {
        let jar = std::env::temp_dir().join(format!(
            "darkclient-discovery-test-{}.jar",
            std::process::id()
        ));
        {
            let mut zip = zip::ZipWriter::new(File::create(&jar).unwrap());
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("net/minecraft/client/Minecraft.class", options)
                .unwrap();
            zip.start_file("version.json", options).unwrap();
            zip.write_all(br#"{ "id": "26.3", "name": "26.3", "world_version": 5023 }"#)
                .unwrap();
            zip.finish().unwrap();
        }
        let class_path = std::env::join_paths([Path::new("other.jar"), jar.as_path()]).unwrap();
        let launch = args(&[
            "-cp",
            class_path.to_str().unwrap(),
            "net.minecraft.client.main.Main",
        ]);

        assert_eq!(detect_version(&launch), Some("26.3".to_string()));
        let _ = std::fs::remove_file(&jar);
    }

    #[test]
    fn the_profile_name_is_not_taken_for_the_version() {
        let launch = args(&[
            "--version",
            "fabric-loader-0.18.6-26.1.1",
            "-cp",
            "missing.jar",
        ]);
        assert_eq!(detect_version(&launch), None);
    }

    #[test]
    fn json_string_fields_are_extracted() {
        let json = r#"{ "id" : "26.4-snapshot-7", "name": "x" }"#;
        assert_eq!(
            json_string_field(json, "id"),
            Some("26.4-snapshot-7".to_string())
        );
        assert_eq!(json_string_field(json, "missing"), None);
    }

    #[test]
    fn mod_loaders_are_recognised() {
        assert_eq!(
            mod_loader(&args(&["net.fabricmc.loader.impl.launch.knot.KnotClient"])),
            "Fabric"
        );
        assert_eq!(
            mod_loader(&args(&["--fml.neoForgeVersion", "26.1.0.1"])),
            "NeoForge"
        );
        assert_eq!(
            mod_loader(&args(&["--fml.forgeVersion", "62.0.0"])),
            "Forge"
        );
        assert_eq!(
            mod_loader(&args(&["net.minecraft.client.main.Main"])),
            "Vanilla"
        );
    }

    #[test]
    fn labels_flag_unsupported_and_unknown_versions() {
        let vanilla = args(&["net.minecraft.client.main.Main"]);
        assert_eq!(label(&vanilla, Some("26.3")), "Minecraft 26.3 · Vanilla");
        assert_eq!(
            label(&vanilla, Some("26.4-snapshot-7")),
            "Minecraft 26.4-snapshot-7 · Vanilla · unsupported"
        );
        assert_eq!(
            label(&vanilla, None),
            "Minecraft (unknown version) · Vanilla"
        );
    }
}
