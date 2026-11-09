//! Client-library lifecycle: load, hot-reload and drop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libloading::Library;
use log::{error, info, warn};
use protocol::{client_abi, Response};

/// Internal result type — agent-side errors are only ever logged.
type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The currently loaded client library, if any.
static CLIENT_LIBRARY: OnceLock<Mutex<Option<Library>>> = OnceLock::new();

/// How long a temporary library copy is kept before deletion.
const TEMP_CLEANUP_DELAY: Duration = Duration::from_secs(5);

/// Locks the client-library slot, recovering from a poisoned mutex rather
/// than panicking — a poisoned lock here would otherwise wedge the agent.
fn lock() -> MutexGuard<'static, Option<Library>> {
    CLIENT_LIBRARY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| {
            error!("client-library mutex was poisoned; recovering");
            poisoned.into_inner()
        })
}

/// Hot-reloads the client library from `source` and reports what became of
/// it. `force` starts the client even on an unsupported Minecraft version.
///
/// The file is copied to a uniquely named temporary path first, so the
/// original can be rebuilt while a copy stays mapped (this matters on
/// Windows, where a loaded DLL is locked on disk).
pub fn reload(source: &Path, force: bool) -> Response {
    info!("reloading client library from {}", source.display());
    let result = copy_to_temp(source).and_then(|temp| {
        schedule_temp_cleanup(temp.clone());
        load(&temp, force)
    });
    result.unwrap_or_else(|e| {
        error!("reload failed: {e}");
        Response::Failed {
            reason: e.to_string(),
        }
    })
}

/// Drops the client library without calling `cleanup_client`. Used during
/// agent teardown, where the JVM is already going away.
pub fn shutdown() {
    if lock().take().is_some() {
        info!("client library dropped");
    }
}

/// Replaces the loaded client library with the one at `path`.
///
/// The previous client is cleaned up and dropped *before* the new one is
/// started, so the old client's hooks are gone before the new client
/// installs its own. A client that refuses to start (unsupported version) or
/// fails is cleaned up and dropped right away. The whole swap holds the lock,
/// so concurrent reloads serialize safely.
fn load(path: &Path, force: bool) -> Result<Response> {
    if !path.exists() {
        return Err(format!("client library not found at {}", path.display()).into());
    }

    let mut slot = lock();

    if let Some(old) = slot.take() {
        cleanup(&old);
        drop(old);
        info!("previous client library unloaded");
    }

    info!("loading client library {}", path.display());
    // SAFETY: loading a trusted shared library built by this project.
    let library = unsafe { Library::new(path)? };

    let response = start(&library, force)?;
    match &response {
        Response::Started { version } => {
            info!("client started on Minecraft {version}");
            *slot = Some(library);
        }
        Response::Unsupported { version } => {
            warn!("client refused Minecraft {version}: unsupported version");
            cleanup(&library);
        }
        Response::Failed { reason } => {
            error!("client failed to start: {reason}");
            cleanup(&library);
        }
    }
    Ok(response)
}

/// Calls the client's start entry point and turns its status into a reply.
fn start(library: &Library, force: bool) -> Result<Response> {
    // SAFETY: the symbol is the client's `start_client`, whose signature the
    // client asserts against `client_abi::StartFn` at compile time.
    let start_fn = unsafe { library.get::<client_abi::StartFn>(client_abi::START_SYMBOL) }
        .map_err(|_| "the client library does not export start_client (outdated build?)")?;
    let mut report = [0u8; client_abi::REPORT_CAPACITY];
    // SAFETY: `report` is valid for `REPORT_CAPACITY` bytes of writes.
    let status = unsafe { start_fn(force, report.as_mut_ptr(), report.len()) };
    let end = report.iter().position(|&b| b == 0).unwrap_or(report.len());
    let report = String::from_utf8_lossy(&report[..end]);
    Ok(Response::from_start(status, &report))
}

/// Calls the client's cleanup entry point, if it exports one.
fn cleanup(library: &Library) {
    // SAFETY: the symbol is the client's `cleanup_client`, whose signature the
    // client asserts against `client_abi::CleanupFn` at compile time.
    match unsafe { library.get::<client_abi::CleanupFn>(client_abi::CLEANUP_SYMBOL) } {
        Ok(cleanup) => {
            info!("calling cleanup_client");
            // SAFETY: see above.
            unsafe { cleanup() };
        }
        Err(_) => info!("cleanup_client not exported; skipping"),
    }
}

/// Copies `source` to a uniquely named file in the system temp directory.
fn copy_to_temp(source: &Path) -> Result<PathBuf> {
    let file_name = source
        .file_name()
        .ok_or("client library path has no file name")?
        .to_string_lossy()
        .into_owned();

    // Nanosecond stamp keeps reloads within the same second from colliding.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut temp = std::env::temp_dir();
    temp.push(format!("dark_client_{stamp}_{file_name}"));

    std::fs::copy(source, &temp)?;
    info!("client library copied to {}", temp.display());
    Ok(temp)
}

/// Spawns a thread that deletes a temporary library copy after a short delay.
fn schedule_temp_cleanup(temp: PathBuf) {
    thread::spawn(move || {
        thread::sleep(TEMP_CLEANUP_DELAY);
        if let Err(e) = std::fs::remove_file(&temp) {
            warn!("could not remove temporary library {}: {e}", temp.display());
        }
    });
}
