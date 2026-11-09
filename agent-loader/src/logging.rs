//! Agent logger setup.

use log::LevelFilter;
use std::path::Path;
use std::sync::Once;

/// Name of the agent's log file.
const LOG_FILE: &str = "agent_loader.log";

static INIT: Once = Once::new();

/// Initializes file logging in `dir` — the injector's working directory, so
/// nothing is written into `.minecraft`. Runs at most once (later calls are
/// no-ops) and never panics — a logging failure must not stop the agent.
pub fn init(dir: &Path) {
    INIT.call_once(|| {
        let path = dir.join(LOG_FILE);
        if let Err(e) = protocol::init_file_logger(&path, LevelFilter::Debug) {
            eprintln!("[agent_loader] file logging disabled: {e}");
        }
    });
}
