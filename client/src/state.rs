//! Global client state: the JNI runtime (installed first, it also reads the
//! game version) and the game / module [`Client`]. The accessors `expect` the
//! state to exist: using one before `init` is a bug.

use std::sync::OnceLock;

use jni::JNIEnv;
use mc_runtime::Runtime;

use mc_bindings::{Minecraft, Window};

use crate::error::{Error, Result};
use crate::module::registry::ModuleRegistry;

static CLIENT: OnceLock<Client> = OnceLock::new();

#[cfg(test)]
pub fn init() -> Result<()> {
    init_runtime()?;
    init_client()
}

/// Attaches to the game and reads its version, so the caller can decide
/// whether to go on.
pub fn init_runtime() -> Result<&'static Runtime> {
    Ok(Runtime::install()?)
}

pub fn init_client() -> Result<()> {
    CLIENT
        .set(Client::new()?)
        .map_err(|_| Error::AlreadyInitialized)
}

#[inline]
pub fn client() -> &'static Client {
    CLIENT.get().expect("client() used before client init")
}

/// `None` before `init` succeeded; the panic hook can get to cleanup that early.
#[inline]
pub fn try_client() -> Option<&'static Client> {
    CLIENT.get()
}

#[inline]
pub fn minecraft() -> &'static Minecraft {
    &client().minecraft
}

#[inline]
pub fn window() -> &'static Window {
    &client().window
}

/// This thread's JNI environment, attaching it the first time.
pub fn env() -> Result<JNIEnv<'static>> {
    Ok(mc_runtime::runtime()?.env()?)
}

/// Releases the JVM references we hold, before unload. The `Minecraft` and
/// `Window` handles stay in their `OnceLock`: two global refs per reload.
pub fn teardown() {
    if let Ok(runtime) = mc_runtime::runtime() {
        // A handler left on the pipeline would crash the JVM after unload.
        crate::net::teardown();
        runtime.teardown();
    }
}

pub struct Client {
    minecraft: Minecraft,
    window: Window,
    pub modules: ModuleRegistry,
}

impl Client {
    fn new() -> Result<Client> {
        let minecraft = Minecraft::get_instance()?;
        let window = minecraft.get_window()?;
        Ok(Client {
            minecraft,
            window,
            modules: ModuleRegistry::new(),
        })
    }
}
