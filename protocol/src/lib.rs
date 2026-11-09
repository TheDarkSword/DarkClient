//! Shared IPC contract between the `injector` and the `agent_loader`.
//!
//! The two processes speak a tiny line-based protocol over a localhost TCP
//! socket: the injector sends a [`Command`], the agent answers with a
//! [`Response`]. Keeping the wire format, the socket address, the agent ↔
//! client ABI and the logger setup in a single crate stops the ends from
//! silently drifting apart.

pub mod client_abi;
mod command;
mod logging;
mod response;

pub use command::{Command, ProtocolError};
pub use logging::{init_file_logger, LoggerError};
pub use response::Response;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Address the `agent_loader` command server binds and the `injector`
/// connects to. Localhost-only by design — the channel is never exposed off
/// the machine.
pub const SOCKET_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878);
