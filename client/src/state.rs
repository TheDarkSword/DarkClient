//! Global client state.
//!
//! Two things live for the whole life of the injected client: the JNI
//! [`Mapping`] bridge and the running game / module [`Client`]. Each is built
//! once — [`init_mapping`] then [`init_client`] — and reached afterwards
//! through a free accessor.
//!
//! Initialization order is explicit (see [`init`]): the mapping is built
//! first because the game wrappers resolve their classes through it. The
//! accessors `expect` the state to exist — using one before `init` is a
//! programmer error, not a runtime condition, so it panics with a clear
//! message instead of returning an `Option` every caller would unwrap.

use std::sync::OnceLock;

use jni::JNIEnv;
use thiserror::Error;

use crate::mapping::client::minecraft::Minecraft;
use crate::mapping::Mapping;
use crate::module::registry::ModuleRegistry;

/// The JNI mapping bridge. Built first; the game wrappers depend on it.
static MAPPING: OnceLock<Mapping> = OnceLock::new();
/// The running game and module state.
static CLIENT: OnceLock<Client> = OnceLock::new();

/// Something that went wrong bringing the client up.
#[derive(Debug, Error)]
pub enum ClientError {
    /// [`init`] was called more than once.
    #[error("the client is already initialized")]
    AlreadyInitialized,
    /// A construction step failed.
    #[error(transparent)]
    Init(#[from] anyhow::Error),
}

/// Initializes the global client state in one go: [`init_mapping`], then
/// [`init_client`]. Must be called exactly once, before any accessor is used.
pub fn init() -> Result<(), ClientError> {
    init_mapping()?;
    init_client()
}

/// First step: builds the JNI mapping bridge — which also reads the running
/// Minecraft version, so the caller can decide whether to go on.
pub fn init_mapping() -> Result<&'static Mapping, ClientError> {
    MAPPING
        .set(Mapping::new()?)
        .map_err(|_| ClientError::AlreadyInitialized)?;
    Ok(mapping())
}

/// Second step: builds the game/module state. [`init_mapping`] must have
/// succeeded.
pub fn init_client() -> Result<(), ClientError> {
    CLIENT
        .set(Client::new()?)
        .map_err(|_| ClientError::AlreadyInitialized)?;
    Ok(())
}

/// The JNI mapping bridge. Valid once [`init`] has succeeded.
#[inline]
pub fn mapping() -> &'static Mapping {
    MAPPING.get().expect("mapping() used before client init")
}

/// The global client. Valid once [`init`] has succeeded.
#[inline]
pub fn client() -> &'static Client {
    CLIENT.get().expect("client() used before client init")
}

/// The global client, or `None` if [`init`] never ran (or failed). Used by
/// `cleanup_client`, which may run from the panic hook before `init` had a
/// chance to succeed.
#[inline]
pub fn try_client() -> Option<&'static Client> {
    CLIENT.get()
}

/// The running Minecraft game. Valid once [`init`] has succeeded.
#[inline]
pub fn minecraft() -> &'static Minecraft {
    &client().minecraft
}

/// Attaches the current thread to the JVM and returns a JNI environment.
pub fn env() -> anyhow::Result<JNIEnv<'static>> {
    mapping().get_env()
}

/// Releases the JVM resources the global state holds — called from
/// `cleanup_client` before the library is unloaded.
///
/// The `MAPPING` / `CLIENT` statics are `OnceLock`s and are never dropped, so
/// the handful of global references kept in plain fields (`Minecraft` and
/// `Window`) outlive a hot-reload. Those point at session-lifetime singletons,
/// so only a couple of global-ref table slots leak per reload; everything
/// sizable — the class-handle cache, the game class loader, the cached player
/// — is released here.
pub fn teardown() {
    // Remove the Netty pipeline handler first — leaving it in place would
    // crash the JVM once this library is unloaded. It can only exist once the
    // mapping does, and needs it.
    if MAPPING.get().is_some() {
        crate::net::teardown();
    }
    if let Some(client) = CLIENT.get() {
        client.minecraft.teardown();
    }
    if let Some(mapping) = MAPPING.get() {
        mapping.teardown();
    }
}

/// The running game and module state.
pub struct Client {
    minecraft: Minecraft,
    /// Every registered module.
    pub modules: ModuleRegistry,
}

impl Client {
    /// Builds the client. The [`MAPPING`] global must already be set.
    fn new() -> Result<Client, ClientError> {
        Ok(Client {
            minecraft: Minecraft::new()?,
            modules: ModuleRegistry::new(),
        })
    }
}
