//! The JNI runtime the generated Minecraft bindings run on.

mod cx;
mod error;
mod loader;
mod object;
mod runtime;
mod verify;

pub use cx::{Cx, Field, Method};
pub use error::{Error, Result};
pub use object::{JavaClass, JavaType, Object};
pub use runtime::{runtime, Runtime};
pub use verify::{BindingCheck, BindingReport, CheckKind};

pub use mc_versions::MinecraftVersion;
