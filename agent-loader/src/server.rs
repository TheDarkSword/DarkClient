//! TCP command server.

use std::io::ErrorKind;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use log::{error, info};
use protocol::SOCKET_ADDR;

use crate::{command, is_running};

/// Idle pause between `accept` polls while no client is connecting.
const ACCEPT_IDLE: Duration = Duration::from_millis(100);
/// Back-off after an unexpected `accept` error, to avoid a busy loop.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_secs(1);

/// Spawns the command-server thread.
pub fn start() {
    thread::spawn(run);
}

/// Accepts connections until the agent shuts down, handling each on its own
/// thread.
fn run() {
    let listener = match TcpListener::bind(SOCKET_ADDR) {
        Ok(listener) => {
            info!("command server listening on {SOCKET_ADDR}");
            listener
        }
        Err(e) => {
            error!("could not bind command server to {SOCKET_ADDR}: {e}");
            return;
        }
    };

    if let Err(e) = listener.set_nonblocking(true) {
        error!("could not make the command socket non-blocking: {e}");
        return;
    }

    while is_running() {
        match listener.accept() {
            Ok((stream, _)) => {
                thread::spawn(move || command::handle_connection(stream));
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_IDLE);
            }
            Err(e) => {
                error!("command server accept error: {e}");
                thread::sleep(ACCEPT_ERROR_BACKOFF);
            }
        }
    }

    info!("command server stopped");
}
