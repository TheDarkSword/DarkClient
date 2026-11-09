//! Windows graphics platform glue.

use std::ffi::{c_void, CString};
use std::sync::OnceLock;

use libloading::Library;
use log::info;

use super::HookTarget;

/// Shared library that exports the OpenGL entry points.
const GL_LIBRARY: &str = "opengl32.dll";

/// Lazily-opened, process-lifetime handle to the OpenGL library.
fn gl_library() -> Option<&'static Library> {
    static LIB: OnceLock<Option<Library>> = OnceLock::new();
    // SAFETY: opening the GL library the host process already maps.
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
        // Modern extension entry points are reachable only through
        // wglGetProcAddress; opengl32.dll itself exports just the GL 1.1 core.
        if let Ok(c_name) = CString::new(name) {
            type WglGetProcAddress = unsafe extern "system" fn(*const i8) -> *const c_void;
            if let Ok(wgl) = lib.get::<WglGetProcAddress>(b"wglGetProcAddress") {
                let ptr = wgl(c_name.as_ptr());
                if !ptr.is_null() {
                    return ptr;
                }
            }
        }
        match lib.get::<unsafe extern "system" fn()>(name.as_bytes()) {
            Ok(symbol) => *symbol as *const c_void,
            Err(_) => std::ptr::null(),
        }
    }
}

/// Opens the GLFW shared library, trying the names Minecraft launchers use.
pub fn open_glfw_library() -> Option<Library> {
    // SAFETY: loading the GLFW DLL the host process already maps.
    unsafe {
        Library::new("glfw.dll")
            .or_else(|_| Library::new("glfw3.dll"))
            .or_else(|_| Library::new("glfw64.dll"))
            .ok()
    }
}

/// Shared library of SDL3, the windowing layer of Minecraft 26.3+.
const SDL_LIBRARY: &str = "SDL3.dll";

/// Opens SDL3 only if the host already loaded it — never pulls in a stray
/// copy from the DLL search path on versions that still use GLFW.
pub fn open_sdl_library() -> Option<Library> {
    libloading::os::windows::Library::open_already_loaded(SDL_LIBRARY)
        .ok()
        .map(Library::from)
}

/// The buffer-swap functions to hook: `SDL_GL_SwapWindow` when the game runs
/// on SDL3, then `wglSwapBuffers` from `opengl32.dll`.
pub fn frame_hook_targets() -> Vec<HookTarget> {
    let mut targets = Vec::new();
    for (library, symbol) in [
        (open_sdl_library(), "SDL_GL_SwapWindow"),
        // SAFETY: opening the GL library the host process already maps.
        (unsafe { Library::new(GL_LIBRARY).ok() }, "wglSwapBuffers"),
    ] {
        let Some(library) = library else {
            continue;
        };
        // SAFETY: resolving an exported function by name.
        let address = match unsafe { library.get::<unsafe extern "system" fn()>(symbol.as_bytes()) }
        {
            Ok(function) => *function as *const () as usize,
            Err(_) => continue,
        };
        info!("found {symbol} at 0x{address:x}");
        // Keep the library handle alive for the process lifetime.
        std::mem::forget(library);
        targets.push(HookTarget {
            address,
            label: symbol.to_string(),
        });
    }
    targets
}
