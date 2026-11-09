//! High-level injection orchestration.
//!
//! Injects the agent into the target process (platform-specific) and then
//! tells it, over the localhost TCP channel, to load or hot-reload the
//! client library — which answers whether it started.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use log::info;
use protocol::{Command, Response, SOCKET_ADDR};

use crate::platform::{AgentInjector, InjectError, PlatformInjector};

/// Base names of the two shared libraries shipped alongside the injector.
const AGENT_BASE: &str = "libagent_loader";
const CLIENT_BASE: &str = "libclient";

/// Total time to keep retrying the connection to the freshly started agent.
const CONNECT_DEADLINE: Duration = Duration::from_secs(5);
/// Timeout of a single connection attempt within that deadline.
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(500);
/// Pause between connection attempts.
const CONNECT_RETRY_PAUSE: Duration = Duration::from_millis(100);
/// Timeout for writing the command once connected.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the client gets to load and report back.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Injects the agent into `pid` (unless already present) and triggers a
/// client (re)load; returns the Minecraft version the client started on.
/// Blocking — callers run it off the UI thread.
///
/// `version` is what discovery read off the process, if anything: a known
/// unsupported version is refused before touching the game. Unless `force`
/// is set, the client then checks the version again from inside the game.
pub fn inject(pid: u32, version: Option<&str>, force: bool) -> Result<String, InjectError> {
    if let Some(version) = version {
        if !force && !mc_versions::is_supported(version) {
            return Err(InjectError::UnsupportedVersion(version.to_string()));
        }
    }

    let injector = PlatformInjector;

    let agent_file = library_file_name(AGENT_BASE);
    let agent_path = locate_library(&agent_file)?;
    let client_path = locate_library(&library_file_name(CLIENT_BASE))?;

    if injector.is_agent_loaded(pid, &agent_file) {
        info!("agent already present in pid {pid}; reloading client only");
    } else {
        injector.inject(pid, &agent_path)?;
    }

    match send_reload(&client_path, force)? {
        Response::Started { version } => Ok(version),
        Response::Unsupported { version } => Err(InjectError::UnsupportedVersion(version)),
        Response::Failed { reason } => Err(InjectError::ClientFailed(reason)),
    }
}

/// Connects to the agent's command server, sends a [`Command::Reload`] and
/// waits for its [`Response`].
fn send_reload(client_lib: &Path, force: bool) -> Result<Response, InjectError> {
    let absolute = std::path::absolute(client_lib).map_err(InjectError::Path)?;
    // The client keeps its config in the directory the injector runs from.
    let config_dir = std::env::current_dir().map_err(InjectError::Path)?;

    let mut stream = connect_with_retry()?;
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));

    let command = Command::Reload {
        library: absolute,
        config_dir,
        force,
    }
    .encode();
    info!("sending command: {command}");
    stream
        .write_all(format!("{command}\n").as_bytes())
        .map_err(InjectError::Send)?;

    let _ = stream.set_read_timeout(Some(REPLY_TIMEOUT));
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| InjectError::NoReply(e.to_string()))?;
    info!("agent replied: {}", reply.trim());
    Response::decode(&reply).map_err(|e| InjectError::NoReply(e.to_string()))
}

/// Repeatedly tries to connect until [`CONNECT_DEADLINE`] elapses — this
/// covers the short window between injecting the agent and its TCP server
/// becoming reachable, without a blind fixed sleep.
fn connect_with_retry() -> Result<TcpStream, InjectError> {
    let deadline = Instant::now() + CONNECT_DEADLINE;
    loop {
        match TcpStream::connect_timeout(&SOCKET_ADDR, CONNECT_ATTEMPT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(source) => {
                if Instant::now() >= deadline {
                    return Err(InjectError::Connect {
                        addr: SOCKET_ADDR,
                        source,
                    });
                }
                thread::sleep(CONNECT_RETRY_PAUSE);
            }
        }
    }
}

/// `libfoo` → `libfoo.so` / `libfoo.dll` / `libfoo.dylib` for the host OS.
fn library_file_name(base: &str) -> String {
    format!("{base}.{}", std::env::consts::DLL_EXTENSION)
}

/// Looks for `file_name` next to the injector executable first, then in the
/// current working directory.
fn locate_library(file_name: &str) -> Result<PathBuf, InjectError> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(file_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    let cwd_candidate = PathBuf::from(file_name);
    if cwd_candidate.is_file() {
        return Ok(cwd_candidate);
    }
    Err(InjectError::LibraryMissing(file_name.to_string()))
}
