//! Platform-specific agent hooks — currently just Unix signal handling.

/// Installs handlers that shut the agent down cleanly on process
/// termination. A no-op on platforms without Unix signals.
pub fn install_signal_handlers() {
    #[cfg(unix)]
    unix::install();
}

#[cfg(unix)]
mod unix {
    use std::sync::atomic::{AtomicBool, Ordering};

    use log::info;

    /// Guards against installing the handlers more than once.
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Installs `SIGTERM` / `SIGINT` handlers.
    pub fn install() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }

        // The handler runs the normal teardown and exits. This mirrors the
        // process shutdown path; it is not strictly async-signal-safe, but
        // the process is terminating regardless.
        extern "C" fn handle_signal(_sig: libc::c_int) {
            crate::shutdown();
            std::process::exit(0);
        }

        // SAFETY: `signal` is a standard libc call; `handle_signal` has the
        // required `extern "C" fn(c_int)` signature.
        unsafe {
            libc::signal(
                libc::SIGTERM,
                handle_signal as *const () as libc::sighandler_t,
            );
            libc::signal(
                libc::SIGINT,
                handle_signal as *const () as libc::sighandler_t,
            );
        }

        info!("signal handlers installed");
    }
}
