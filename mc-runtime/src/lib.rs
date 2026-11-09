//! The JNI runtime under DarkClient's Minecraft bindings.
//!
//! [`Runtime`] owns what lives for the whole injected session: the JVM handle,
//! the class loader that runs the game (discovered on Fabric / Forge, captured
//! on vanilla), the resolved-class cache and the running Minecraft version.
//! [`Cx`] is a JNI environment paired with its runtime — what every binding
//! call takes. [`Method`] and [`Field`] are the call targets the generated
//! bindings declare; each caches its JNI id after the first use.

mod cx;
mod error;
mod loader;
mod runtime;

pub use cx::{Cx, Field, Method};
pub use error::{Error, Result};
pub use runtime::Runtime;

pub use mc_versions::MinecraftVersion;
