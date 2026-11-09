//! Agent loader — a `cdylib` injected into the Minecraft JVM.
//!
//! On load it starts a JVM health monitor and a TCP command server, and it
//! owns the lifecycle of the client library (load / hot-reload / unload).

mod command;
mod jvm;
mod library;
mod logging;
mod platform;
mod server;

use std::sync::atomic::{AtomicBool, Ordering};

use ctor::{ctor, dtor};
use log::info;

/// Cleared on shutdown; every background loop watches it to know when to stop.
static RUNNING: AtomicBool = AtomicBool::new(true);

/// Whether the agent is still running.
pub(crate) fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// Runs automatically when the agent library is loaded into the JVM process.
#[ctor]
fn agent_onload() {
    // File logging is set up once the first command reveals where to write it
    // (the injector's directory) — see `command::handle_connection`.
    info!("agent loader initialized");

    platform::install_signal_handlers();
    jvm::start_monitor();
    server::start();
}

/// Runs automatically when the agent library is unloaded.
#[dtor]
fn agent_onunload() {
    shutdown();
}

/// Idempotent shutdown: stops the background loops and drops the client
/// library. Safe to call from a signal handler, the JVM monitor or the dtor.
pub(crate) fn shutdown() {
    if !RUNNING.swap(false, Ordering::SeqCst) {
        return; // already shut down
    }
    info!("agent loader shutting down");
    library::shutdown();
}
