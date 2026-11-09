//! Platform-specific graphics glue: OpenGL entry-point resolution, the GLFW
//! and SDL3 shared libraries, and the buffer-swap hook targets.
//!
//! Linux and Windows are real. macOS resolves the libraries but provides no
//! frame-hook target — `ilhook` is x86-64 only — so the overlay does not
//! install there yet. Adding macOS support means editing only `macos.rs`.

use std::ffi::c_void;

use libloading::Library;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod imp;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[path = "macos.rs"]
mod imp;

/// A candidate location for the buffer-swap hook: a resolved code address and
/// a human-readable label for logging.
pub struct HookTarget {
    /// Absolute address of the function to hook.
    pub address: usize,
    /// Where it came from — shown in logs.
    pub label: String,
}

/// Resolves an OpenGL function pointer by name — the loader fed to `gl` and
/// `glow`. Returns null when the symbol cannot be found.
pub fn gl_proc_address(name: &str) -> *const c_void {
    // Under SDL3 the context may be EGL (Wayland), which the system GL
    // library cannot resolve for — SDL knows which one it created.
    if let Some(sdl) = crate::graphic::sdl::api() {
        let address = sdl.gl_proc_address(name);
        if !address.is_null() {
            return address;
        }
    }
    imp::gl_proc_address(name)
}

/// Opens the GLFW shared library the host process uses, if it can be found.
pub fn open_glfw_library() -> Option<Library> {
    imp::open_glfw_library()
}

/// Opens the SDL3 shared library, only if the host process already loaded it
/// (Minecraft 26.3+).
pub fn open_sdl_library() -> Option<Library> {
    imp::open_sdl_library()
}

/// The buffer-swap functions to try hooking, most specific first.
pub fn frame_hook_targets() -> Vec<HookTarget> {
    imp::frame_hook_targets()
}
