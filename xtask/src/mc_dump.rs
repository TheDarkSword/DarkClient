//! `cargo xtask mc-dump` — writes `mc-data/dumps/<version>.txt` for one or
//! more Minecraft versions.
//!
//! ```text
//! cargo xtask mc-dump 26.3 26.2        # download (cached) and dump
//! cargo xtask mc-dump --supported      # every supported version
//! cargo xtask mc-dump 26.4 --jar x.jar # dump a local client jar instead
//! ```

use std::path::{Path, PathBuf};

use mc_bindgen::{classfile, fetch};

const USAGE: &str = "usage: cargo xtask mc-dump (<version>... | --supported) [--jar <client.jar>]";

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let mut versions: Vec<String> = Vec::new();
    let mut jar: Option<PathBuf> = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--supported" => versions.extend(
                mc_versions::SUPPORTED_VERSIONS
                    .iter()
                    .map(|v| v.to_string()),
            ),
            "--jar" => {
                let path = args.next().ok_or(USAGE)?;
                jar = Some(PathBuf::from(path));
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}; {USAGE}")),
            version => versions.push(version.to_string()),
        }
    }
    if versions.is_empty() {
        return Err(USAGE.to_string());
    }
    if jar.is_some() && versions.len() != 1 {
        return Err("--jar dumps exactly one version".to_string());
    }

    let roots = read_roots(&root.join("mc-data/classes.txt"))?;
    let dumps = root.join("mc-data/dumps");
    std::fs::create_dir_all(&dumps).map_err(|e| format!("{}: {e}", dumps.display()))?;
    let cache = root.join("target/mc-cache");

    for version in &versions {
        let (jar_path, sha1) = match &jar {
            Some(path) => (
                path.clone(),
                fetch::sha1_of(path).map_err(|e| e.to_string())?,
            ),
            None => {
                println!("[{version}] fetching the client jar…");
                fetch::client_jar(version, &cache).map_err(|e| e.to_string())?
            }
        };
        let dump =
            classfile::dump_jar(&jar_path, version, &sha1, &roots).map_err(|e| e.to_string())?;
        let out = dumps.join(format!("{version}.txt"));
        std::fs::write(&out, dump.to_text()).map_err(|e| format!("{}: {e}", out.display()))?;
        println!(
            "[{version}] {} classes, {} missing roots → {}",
            dump.classes.len(),
            dump.missing.len(),
            out.strip_prefix(root).unwrap_or(&out).display()
        );
    }
    Ok(())
}

/// The root class list: one JNI class name per line, `#` comments.
fn read_roots(path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}
