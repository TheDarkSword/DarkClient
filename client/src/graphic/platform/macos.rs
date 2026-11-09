//! macOS — and any other non-Linux/Windows target — graphics platform glue.
//!
//! OpenGL and GLFW library resolution are provided. The buffer-swap hook is
//! not: `ilhook` is x86-64 only, so [`frame_hook_targets`] returns nothing
//! and the overlay does not install on macOS yet. Implementing it (an ARM64
//! Mach-O hook, or a different interception point) means editing only this
//! file — nothing else in the crate is platform-aware.

use std::ffi::c_void;
use std::sync::OnceLock;

use libloading::Library;
use log::warn;

use super::HookTarget;

/// The system OpenGL framework binary.
const GL_LIBRARY: &str = "/System/Library/Frameworks/OpenGL.framework/Versions/Current/OpenGL";

/// Lazily-opened, process-lifetime handle to the OpenGL framework.
fn gl_library() -> Option<&'static Library> {
    static LIB: OnceLock<Option<Library>> = OnceLock::new();
    // SAFETY: opening the system OpenGL framework.
    LIB.get_or_init(|| unsafe { Library::new(GL_LIBRARY).ok() })
        .as_ref()
}

/// Resolves an OpenGL function pointer by name.
pub fn gl_proc_address(name: &str) -> *const c_void {
    let Some(lib) = gl_library() else {
        return std::ptr::null();
    };
    // SAFETY: resolving an exported GL symbol by name.
    unsafe {
        match lib.get::<unsafe extern "system" fn()>(name.as_bytes()) {
            Ok(symbol) => *symbol as *const c_void,
            Err(_) => std::ptr::null(),
        }
    }
}

/// Opens the GLFW shared library the host loaded.
pub fn open_glfw_library() -> Option<Library> {
    // SAFETY: loading the GLFW dylib the host process already maps.
    unsafe {
        Library::new("libglfw.3.dylib")
            .or_else(|_| Library::new("libglfw.dylib"))
            .ok()
    }
}

/// Opens the SDL3 dylib only if the host already loaded it.
pub fn open_sdl_library() -> Option<Library> {
    // SAFETY: RTLD_NOLOAD never loads anything new; it only returns a handle
    // to a dylib the host process already maps.
    unsafe {
        libloading::os::unix::Library::open(
            Some("libSDL3.0.dylib"),
            libloading::os::unix::RTLD_LAZY | libc::RTLD_NOLOAD,
        )
        .ok()
        .map(Library::from)
    }
}

/// No frame-hook target is available on macOS yet — see the module docs.
pub fn frame_hook_targets() -> Vec<HookTarget> {
    warn!("frame hooking is not implemented on macOS; the overlay will not install");
    Vec::new()
}
