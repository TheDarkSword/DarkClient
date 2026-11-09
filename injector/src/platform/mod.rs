//! Platform abstraction for process discovery and agent injection.
//!
//! Each OS provides an [`AgentInjector`] implementation. Linux and Windows
//! are real; macOS is a stub today, but the seam lives here so that adding
//! real macOS support means editing only `macos.rs`.

mod discovery;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod imp;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[path = "macos.rs"]
mod imp;

pub use discovery::{find_minecraft_processes, ProcessInfo};
pub use imp::PlatformInjector;

use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;

use thiserror::Error;

/// A boxed, thread-safe source error. Lets each platform funnel its own
/// backend error type (ptrace, dll-syringe, …) into [`InjectError`] without
/// the abstraction depending on those crates.
pub type BoxError = Box<dyn Error + Send + Sync>;

/// Everything that can go wrong while injecting the agent and triggering a
/// client load. `Send + Sync` so it can be returned from a worker thread.
#[derive(Debug, Error)]
pub enum InjectError {
    /// The target process disappeared before injection could start.
    #[error("process {0} is no longer running")]
    ProcessGone(u32),
    /// Attaching to the target process failed.
    #[error("could not attach to process {pid}: {source}")]
    Attach { pid: u32, source: BoxError },
    /// The platform backend failed to map the agent library in.
    #[error("agent injection failed: {0}")]
    Inject(BoxError),
    /// A required shared library could not be found on disk.
    #[error("library not found next to the injector or in the working directory: {0}")]
    LibraryMissing(String),
    /// The agent's command server could not be reached.
    #[error("could not reach the agent on {addr}: {source}")]
    Connect {
        addr: SocketAddr,
        source: std::io::Error,
    },
    /// Writing the command to the agent failed.
    #[error("failed to send the reload command: {0}")]
    Send(std::io::Error),
    /// An absolute path could not be resolved.
    #[error("could not resolve an absolute library path: {0}")]
    Path(std::io::Error),
    /// The Minecraft version is not one DarkClient supports.
    #[error(
        "Minecraft {0} is not supported (supported: {supported}); tick \"Ignore version check\" to inject anyway",
        supported = mc_versions::SUPPORTED_VERSIONS.join(", ")
    )]
    UnsupportedVersion(String),
    /// The client library loaded but could not start.
    #[error("the client failed to start: {0}")]
    ClientFailed(String),
    /// The agent did not answer the reload command.
    #[error("no reply from the agent ({0}); if Minecraft was started before updating DarkClient, restart it")]
    NoReply(String),
    /// Injection is not implemented for the host platform.
    // Constructed only by the macOS / fallback backend, so it reads as dead
    // code on Linux and Windows builds.
    #[allow(dead_code)]
    #[error("agent injection is not supported on this platform yet")]
    Unsupported,
}

/// Platform-specific agent injection. One implementation per OS.
pub trait AgentInjector {
    /// Whether a shared library named `agent_file` is already mapped into
    /// the process `pid`.
    fn is_agent_loaded(&self, pid: u32, agent_file: &str) -> bool;

    /// Injects the agent shared library at `agent_path` into `pid`.
    fn inject(&self, pid: u32, agent_path: &Path) -> Result<(), InjectError>;
}

/// Whether the current process holds the privileges injection requires.
pub fn is_elevated() -> bool {
    imp::is_elevated()
}
