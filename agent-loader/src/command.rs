//! Command parsing and dispatch for a single client connection.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use log::{error, info};
use protocol::Command;

use crate::{library, logging};

/// Maximum time spent waiting for a command line before giving up.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads one command off `stream` and executes it. Runs on its own thread,
/// so a slow reload never blocks later connections.
pub fn handle_connection(stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line) {
        error!("failed to read command: {e}");
        return;
    }

    let response = match Command::decode(&line) {
        Ok(Command::Reload {
            library,
            config_dir,
            force,
        }) => {
            // The injector's working directory is where both this agent and
            // the client keep their files — set up logging there (keeping
            // `.minecraft` clean) and hand it to the client, loaded into this
            // same process, through the environment.
            if !config_dir.as_os_str().is_empty() {
                logging::init(&config_dir);
                std::env::set_var("DARK_CONFIG_DIR", &config_dir);
            }
            info!(
                "reload command received: {}{}",
                library.display(),
                if force { " (forced)" } else { "" }
            );
            library::reload(&library, force)
        }
        Err(e) => {
            error!("ignoring invalid command {:?}: {e}", line.trim());
            return;
        }
    };

    // An injector that does not wait for the reply just closed the socket;
    // that is fine.
    let reply = format!("{}\n", response.encode());
    if let Err(e) = reader.get_mut().write_all(reply.as_bytes()) {
        info!("could not send the reply to the injector: {e}");
    }
}
