//! Workspace developer tasks. Run via `cargo xtask <task>`.
//!
//! Tasks:
//!   e2e      End-to-end harness — build everything and inject into a running
//!            Minecraft. This is the manual Tier-3 test: it is not run in CI.
//!   mc-dump  Write `mc-data/dumps/<version>.txt` from a version's client jar
//!            (see `mc_dump.rs`).

mod mc_dump;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("e2e") => e2e(),
        Some("mc-dump") => mc_dump::run(&workspace_root(), &args[1..]),
        Some(other) => Err(format!("unknown task `{other}` (try: e2e, mc-dump)")),
        None => Err("usage: cargo xtask <task>  (tasks: e2e, mc-dump)".to_string()),
    };
    if let Err(message) = result {
        eprintln!("xtask: {message}");
        std::process::exit(1);
    }
}

/// Workspace root — the parent of the `xtask/` crate directory.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always has a parent directory")
        .to_path_buf()
}

/// End-to-end harness.
///
/// Builds the workspace, then injects the client into a running Minecraft.
/// Minecraft must already be running; on Linux this task must run as root
/// (injection uses `ptrace`). Run it with, for example, `sudo -E cargo xtask
/// e2e`. The final overlay check is manual — see the printed instructions.
fn e2e() -> Result<(), String> {
    let root = workspace_root();

    println!("[1/4] building the workspace (release)…");
    build_release(&root)?;

    let release = root.join("target/release");
    let injector = release.join(injector_file_name());
    if !injector.is_file() {
        return Err(format!(
            "injector binary not found at {}",
            injector.display()
        ));
    }
    // libagent_loader / libclient sit next to the injector in target/release,
    // which is exactly where the injector looks for them.
    println!("[2/4] artifacts staged in {}", release.display());

    println!("[3/4] discovering running Minecraft instances…");
    let processes = list_minecraft(&injector)?;
    let Some((pid, info)) = processes.first() else {
        return Err("no running Minecraft found — start the game, then re-run".to_string());
    };
    println!("      found PID {pid} — {info}");

    println!("[4/4] injecting into PID {pid}…");
    let status = Command::new(&injector)
        .arg("--inject")
        .arg(pid.to_string())
        .current_dir(&release)
        .status()
        .map_err(|e| format!("could not run the injector: {e}"))?;
    if !status.success() {
        return Err("injection failed — see the output above and app.log".to_string());
    }

    println!();
    println!("injection succeeded.");
    println!("verify in-game: focus Minecraft and press Right Shift —");
    println!("the DarkClient menu should open.");
    Ok(())
}

/// `cargo build --release` for the three runtime artifacts.
fn build_release(root: &Path) -> Result<(), String> {
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "build",
            "--release",
            "-p",
            "agent_loader",
            "-p",
            "client",
            "-p",
            "injector",
        ])
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("release build failed".to_string())
    }
}

/// Runs `injector --list` and parses its `pid<TAB>info` lines.
fn list_minecraft(injector: &Path) -> Result<Vec<(u32, String)>, String> {
    let output = Command::new(injector)
        .arg("--list")
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("could not run the injector: {e}"))?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, info) = line.split_once('\t')?;
            Some((pid.trim().parse().ok()?, info.trim().to_string()))
        })
        .collect())
}

/// Platform file name of the injector binary.
fn injector_file_name() -> &'static str {
    if cfg!(windows) {
        "injector.exe"
    } else {
        "injector"
    }
}
