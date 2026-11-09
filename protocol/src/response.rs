//! The agent_loader → injector reply to a `Reload`, and its wire encoding.

use crate::client_abi;
use crate::ProtocolError;

/// What became of a `Reload`, as the agent reports it back on the same
/// connection: one UTF-8 line, a verb and its argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// The client started on Minecraft `version`.
    Started { version: String },
    /// The client refused to start: `version` is not supported.
    Unsupported { version: String },
    /// The client could not be loaded or started.
    Failed { reason: String },
}

impl Response {
    /// Builds the reply from what the client's start entry point returned.
    pub fn from_start(status: i32, report: &str) -> Response {
        match status {
            client_abi::STARTED => Response::Started {
                version: report.to_string(),
            },
            client_abi::UNSUPPORTED => Response::Unsupported {
                version: report.to_string(),
            },
            _ => Response::Failed {
                reason: report.to_string(),
            },
        }
    }

    /// Serializes the response into its single-line wire form (no newline).
    pub fn encode(&self) -> String {
        let (verb, argument) = match self {
            Response::Started { version } => ("started", version),
            Response::Unsupported { version } => ("unsupported", version),
            Response::Failed { reason } => ("failed", reason),
        };
        // The argument runs to the end of the line, so it must stay on one.
        format!("{verb} {}", argument.replace(['\r', '\n'], " "))
    }

    /// Parses one wire line back into a [`Response`].
    pub fn decode(line: &str) -> Result<Self, ProtocolError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ProtocolError::Empty);
        }
        let (verb, argument) = match line.split_once(' ') {
            Some((verb, argument)) => (verb, argument.trim().to_string()),
            None => (line, String::new()),
        };
        match verb {
            "started" => Ok(Response::Started { version: argument }),
            "unsupported" => Ok(Response::Unsupported { version: argument }),
            "failed" => Ok(Response::Failed { reason: argument }),
            other => Err(ProtocolError::UnknownVerb(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_response_round_trips() {
        for response in [
            Response::Started {
                version: "26.3".into(),
            },
            Response::Unsupported {
                version: "26.4-snapshot-7".into(),
            },
            Response::Failed {
                reason: "no JVM found in this process".into(),
            },
        ] {
            assert_eq!(Response::decode(&response.encode()), Ok(response));
        }
    }

    #[test]
    fn a_multi_line_reason_is_flattened() {
        let response = Response::Failed {
            reason: "first\nsecond".into(),
        };
        assert_eq!(response.encode(), "failed first second");
    }

    #[test]
    fn start_statuses_map_to_responses() {
        assert_eq!(
            Response::from_start(client_abi::STARTED, "26.3"),
            Response::Started {
                version: "26.3".into()
            }
        );
        assert_eq!(
            Response::from_start(client_abi::UNSUPPORTED, "26.4"),
            Response::Unsupported {
                version: "26.4".into()
            }
        );
        assert_eq!(
            Response::from_start(42, "boom"),
            Response::Failed {
                reason: "boom".into()
            }
        );
    }

    #[test]
    fn an_unknown_verb_is_rejected() {
        assert_eq!(
            Response::decode("maybe 26.3"),
            Err(ProtocolError::UnknownVerb("maybe".into()))
        );
    }
}
