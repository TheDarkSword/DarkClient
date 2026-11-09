//! Downloading Minecraft client jars from Mojang.
//!
//! Uses the system `curl` (shipped with Linux distributions, macOS and
//! Windows 10+) instead of an HTTP/TLS stack, which would cost far more build
//! time than this occasional download is worth. Jars are verified against the
//! SHA-1 Mojang publishes and cached, so a version is fetched once.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::Error;

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Runs `curl` on `url`; writes to `output` if given, else returns the body.
fn curl(url: &str, output: Option<&Path>) -> Result<Vec<u8>, Error> {
    let mut command = Command::new("curl");
    command.args(["--fail", "--silent", "--show-error", "--location"]);
    if let Some(output) = output {
        command.arg("--output").arg(output);
    }
    let result = command
        .arg(url)
        .output()
        .map_err(|e| Error::Fetch(format!("could not run curl: {e}")))?;
    if !result.status.success() {
        return Err(Error::Fetch(format!(
            "curl {url}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(result.stdout)
}

fn json(url: &str) -> Result<Value, Error> {
    serde_json::from_slice(&curl(url, None)?)
        .map_err(|e| Error::Fetch(format!("{url}: invalid JSON: {e}")))
}

/// Hex SHA-1 of a file.
pub fn sha1_of(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(sha1_smol::Sha1::from(bytes).digest().to_string())
}

/// The client jar of version `id` — from `cache` when present and intact,
/// downloaded otherwise. Returns its path and SHA-1.
pub fn client_jar(id: &str, cache: &Path) -> Result<(PathBuf, String), Error> {
    let manifest = json(VERSION_MANIFEST)?;
    let version_url = manifest["versions"]
        .as_array()
        .and_then(|versions| versions.iter().find(|v| v["id"] == id))
        .and_then(|version| version["url"].as_str())
        .ok_or_else(|| Error::Fetch(format!("version {id} is not in Mojang's manifest")))?
        .to_string();
    let version = json(&version_url)?;
    let client = &version["downloads"]["client"];
    let (Some(url), Some(sha1)) = (client["url"].as_str(), client["sha1"].as_str()) else {
        return Err(Error::Fetch(format!("version {id} has no client download")));
    };

    std::fs::create_dir_all(cache).map_err(|e| Error::io(cache, e))?;
    let jar = cache.join(format!("{id}.jar"));
    if jar.is_file() && sha1_of(&jar)? == sha1 {
        return Ok((jar, sha1.to_string()));
    }

    let partial = cache.join(format!("{id}.jar.part"));
    curl(url, Some(&partial))?;
    let actual = sha1_of(&partial)?;
    if actual != sha1 {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::Fetch(format!(
            "{id}: downloaded jar has SHA-1 {actual}, Mojang lists {sha1}"
        )));
    }
    std::fs::rename(&partial, &jar).map_err(|e| Error::io(&jar, e))?;
    Ok((jar, sha1.to_string()))
}
