use std::sync::LazyLock;

use ash::vk;
use dashmap::DashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Game(#[from] mc_runtime::Error),

    /// `reason` is the Java exception, when the call threw one.
    #[error("{call} failed: {reason}")]
    Jni { call: &'static str, reason: String },

    #[error("{0} is null")]
    Null(&'static str),

    #[error("{what} is not a {expected}")]
    WrongType {
        what: &'static str,
        expected: &'static str,
    },

    #[error("the client is already initialized")]
    AlreadyInitialized,

    #[error("none of the buffer-swap functions could be hooked")]
    NoFrameHook,

    #[error("{0} is not loaded")]
    MissingVulkanFunction(&'static str),

    #[error("{call} failed: {result}")]
    Vulkan {
        call: &'static str,
        result: vk::Result,
    },

    #[error("egui renderer {call} failed: {source}")]
    Renderer {
        call: &'static str,
        source: egui_ash_renderer::RendererError,
    },

    #[error("the Vulkan overlay has no {0}")]
    OverlayState(&'static str),

    #[error("swapchain image {0} is out of range")]
    SwapchainImage(u32),

    #[error("the {0} lock is poisoned")]
    Poisoned(&'static str),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// For a raw JNI call: picks up the Java exception it threw, if any.
    pub fn jni(env: &mut jni::JNIEnv, call: &'static str, error: jni::errors::Error) -> Error {
        let reason = mc_runtime::take_exception(env).unwrap_or_else(|| error.to_string());
        Error::Jni { call, reason }
    }
}

static LOGGED: LazyLock<DashMap<&'static str, String>> = LazyLock::new(DashMap::new);

/// For a failure that would repeat every frame or packet: it is logged only
/// when it differs from the last one logged under `what`.
pub fn warn_changed(what: &'static str, error: &impl std::fmt::Display) {
    let message = error.to_string();
    if LOGGED.get(what).is_some_and(|last| *last == message) {
        return;
    }
    log::warn!("{what}: {message}");
    LOGGED.insert(what, message);
}
