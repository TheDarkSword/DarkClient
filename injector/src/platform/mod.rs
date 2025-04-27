pub const AGENT_NAME: &str = "libagent_loader";
pub const LIBRARY_NAME: &str = "libclient";
pub const SOCKET_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7878);

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(unix)]
pub use self::unix::find_pid;
#[cfg(unix)]
pub use self::unix::inject;

#[cfg(windows)]
pub use self::windows::find_pid;
#[cfg(windows)]
pub use self::windows::inject;
