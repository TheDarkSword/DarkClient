//! The C ABI between the agent loader and the client library it loads.
//!
//! Defined once here so both sides agree on the symbol names, the signature
//! and the status codes. The client asserts its exports against these types.

/// Starts the client: `force` starts it even on an unsupported Minecraft
/// version. Writes a NUL-terminated UTF-8 report into `report` (at most
/// `capacity` bytes, NUL included) — the Minecraft version id, or the failure
/// reason — and returns [`STARTED`], [`UNSUPPORTED`] or [`FAILED`].
pub type StartFn = unsafe extern "C" fn(force: bool, report: *mut u8, capacity: usize) -> i32;

/// Stops the client and releases everything it installed. Safe to call on a
/// client that never started.
pub type CleanupFn = unsafe extern "C" fn();

pub const START_SYMBOL: &[u8] = b"start_client";
pub const CLEANUP_SYMBOL: &[u8] = b"cleanup_client";

/// Size of the report buffer the agent hands to [`StartFn`].
pub const REPORT_CAPACITY: usize = 512;

pub const STARTED: i32 = 0;
pub const UNSUPPORTED: i32 = 1;
pub const FAILED: i32 = 2;
