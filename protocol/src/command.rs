//! The injector → agent_loader command set and its wire encoding.

use std::path::PathBuf;

use thiserror::Error;

/// Third `Reload` field asking the client to start on a version it does not
/// support.
const FORCE_FLAG: &str = "force";

/// A single command sent from the injector to the agent loader.
///
/// The wire form is one UTF-8 line: a lowercase verb followed by a space and
/// its arguments. `Reload` carries two paths — the client library and the
/// injector's working directory — and an optional `force` flag, separated by
/// tabs, so each path may still contain spaces and survive the round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Load — or hot-reload — the client library, and tell the client where
    /// to keep its config (the injector's working directory).
    Reload {
        /// Absolute path of the client library.
        library: PathBuf,
        /// Directory the injector was started in — where the client config
        /// file is read from and written to.
        config_dir: PathBuf,
        /// Start even on a Minecraft version that is not supported — a
        /// development override.
        force: bool,
    },
}

/// Failure to parse a command or a response off the wire.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// The line carried no verb.
    #[error("empty line")]
    Empty,
    /// The verb is not one this protocol version understands.
    #[error("unknown verb: {0:?}")]
    UnknownVerb(String),
    /// A verb that requires an argument was sent without one.
    #[error("{verb:?} is missing its argument")]
    MissingArgument { verb: &'static str },
}

impl Command {
    /// Serializes the command into its single-line wire form (no newline).
    pub fn encode(&self) -> String {
        match self {
            Command::Reload {
                library,
                config_dir,
                force,
            } => {
                let mut line = format!("reload {}\t{}", library.display(), config_dir.display());
                if *force {
                    line.push('\t');
                    line.push_str(FORCE_FLAG);
                }
                line
            }
        }
    }

    /// Parses one wire line back into a [`Command`].
    ///
    /// The argument runs to the end of the line, so paths containing spaces
    /// survive the round trip.
    pub fn decode(line: &str) -> Result<Self, ProtocolError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ProtocolError::Empty);
        }
        let (verb, arg) = match line.split_once(' ') {
            Some((verb, arg)) => (verb, arg.trim()),
            None => (line, ""),
        };
        match verb {
            "reload" => {
                if arg.is_empty() {
                    return Err(ProtocolError::MissingArgument { verb: "reload" });
                }
                // Library, config directory and flags are tab-separated; an
                // old-style line without a tab still decodes (no config dir).
                let mut fields = arg.split('\t');
                let library = PathBuf::from(fields.next().unwrap_or_default());
                let config_dir = PathBuf::from(fields.next().unwrap_or_default());
                let force = fields.any(|flag| flag == FORCE_FLAG);
                Ok(Command::Reload {
                    library,
                    config_dir,
                    force,
                })
            }
            other => Err(ProtocolError::UnknownVerb(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reload_round_trips() {
        let command = Command::Reload {
            library: PathBuf::from("/tmp/libclient.so"),
            config_dir: PathBuf::from("/home/work"),
            force: false,
        };
        let wire = command.encode();
        assert_eq!(wire, "reload /tmp/libclient.so\t/home/work");
        assert_eq!(Command::decode(&wire), Ok(command));
    }

    #[test]
    fn a_forced_reload_round_trips() {
        let command = Command::Reload {
            library: PathBuf::from("/tmp/libclient.so"),
            config_dir: PathBuf::from("/home/work"),
            force: true,
        };
        let wire = command.encode();
        assert_eq!(wire, "reload /tmp/libclient.so\t/home/work\tforce");
        assert_eq!(Command::decode(&wire), Ok(command));
    }

    #[test]
    fn reload_paths_with_spaces_survive_the_round_trip() {
        let command = Command::Reload {
            library: PathBuf::from("/home/My Games/libclient.so"),
            config_dir: PathBuf::from("/home/My Games/cfg dir"),
            force: true,
        };
        assert_eq!(Command::decode(&command.encode()), Ok(command));
    }

    #[test]
    fn decode_ignores_surrounding_whitespace() {
        assert_eq!(
            Command::decode("  reload /a/b.so\t/c\n"),
            Ok(Command::Reload {
                library: PathBuf::from("/a/b.so"),
                config_dir: PathBuf::from("/c"),
                force: false,
            }),
        );
    }

    #[test]
    fn reload_without_a_config_dir_decodes_with_an_empty_one() {
        assert_eq!(
            Command::decode("reload /a/b.so"),
            Ok(Command::Reload {
                library: PathBuf::from("/a/b.so"),
                config_dir: PathBuf::new(),
                force: false,
            }),
        );
    }

    #[test]
    fn an_empty_line_is_rejected() {
        assert_eq!(Command::decode("   "), Err(ProtocolError::Empty));
    }

    #[test]
    fn reload_without_an_argument_is_rejected() {
        assert_eq!(
            Command::decode("reload"),
            Err(ProtocolError::MissingArgument { verb: "reload" }),
        );
    }

    #[test]
    fn an_unknown_verb_is_rejected() {
        assert_eq!(
            Command::decode("frobnicate x"),
            Err(ProtocolError::UnknownVerb("frobnicate".to_string())),
        );
    }
}
