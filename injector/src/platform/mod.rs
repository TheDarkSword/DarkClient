pub const LIBRARY_NAME: &str = "libclient";
pub const SOCKET_ADDRESS: &str = "127.0.0.1:7878";

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use self::unix::find_pid;
#[cfg(unix)]
pub use self::unix::inject;

#[cfg(windows)]
pub use self::windows::find_pid;
#[cfg(windows)]
pub use self::windows::inject;
