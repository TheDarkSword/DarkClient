//! `cargo xtask bindgen` — generates `mc-bindings/src/generated.rs` and
//! `mc-versions/src/supported.rs` from `mc-data/` (spec, adapters, dumps).
//!
//! ```text
//! cargo xtask bindgen           # write the generated files
//! cargo xtask bindgen --check   # fail if they are out of date (CI)
//! ```

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE: &str = "usage: cargo xtask bindgen [--check]";

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => return Err(USAGE.to_string()),
    };
    let generated = mc_bindgen::generate(&root.join("mc-data"))
        .map_err(|e| format!("the spec does not resolve on every dumped version:\n{e}"))?;
    let outputs = [
        (
            root.join("mc-bindings/src/generated.rs"),
            generated.bindings,
        ),
        (
            root.join("mc-versions/src/supported.rs"),
            generated.supported,
        ),
    ];

    let mut stale = Vec::new();
    for (path, source) in outputs {
        let source = rustfmt(&source)?;
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == source {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
        } else {
            std::fs::write(&path, source).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }
    if !stale.is_empty() {
        return Err(format!(
            "out of date — run `cargo xtask bindgen`: {}",
            stale.join(", ")
        ));
    }
    Ok(())
}

/// Formats `source` with the workspace's rustfmt.
fn rustfmt(source: &str) -> Result<String, String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run rustfmt: {e}"))?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(source.as_bytes())
        .map_err(|e| format!("rustfmt: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("rustfmt: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustfmt rejected the generated code:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("rustfmt: {e}"))
}
