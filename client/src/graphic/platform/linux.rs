//! Linux graphics platform glue.

use std::ffi::{c_void, CString};
use std::sync::OnceLock;

use libloading::Library;
use log::info;

use super::HookTarget;

/// Shared library that exports the OpenGL entry points.
const GL_LIBRARY: &str = "libGL.so.1";

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
    // SAFETY: resolving an exported symbol of the GL library by name.
    unsafe {
        match lib.get::<unsafe extern "system" fn()>(name.as_bytes()) {
            Ok(symbol) => *symbol as *const c_void,
            Err(_) => std::ptr::null(),
        }
    }
}

/// Opens the GLFW shared library the host loaded.
pub fn open_glfw_library() -> Option<Library> {
    let path = find_library_path("libglfw.so").unwrap_or_else(|| "libglfw.so".to_string());
    // SAFETY: loading the GLFW library the host process already maps.
    unsafe { Library::new(path).ok() }
}

/// Opens the SDL3 library the host loaded, if any.
pub fn open_sdl_library() -> Option<Library> {
    let path = find_library_path("libSDL3.so")?;
    // SAFETY: loading the SDL library the host process already maps.
    unsafe { Library::new(path).ok() }
}

/// The buffer-swap functions to try hooking, most specific first.
pub fn frame_hook_targets() -> Vec<HookTarget> {
    // (library, exported swap function) candidates, most specific first.
    let mut candidates: Vec<(String, &str)> = Vec::new();
    if let Some(path) = find_library_path("libSDL3.so") {
        info!("found SDL3 library: {path}");
        candidates.push((path, "SDL_GL_SwapWindow"));
    } else if let Some(path) = find_library_path("libglfw.so") {
        info!("found GLFW library: {path}");
        candidates.push((path, "glfwSwapBuffers"));
    } else if let Some(path) = find_library_path("liblwjgl.so") {
        info!("found LWJGL library (legacy): {path}");
        candidates.push((path, "glXSwapBuffers"));
    } else {
        info!("no specific GL library found; falling back to system libGL");
        candidates.push((GL_LIBRARY.to_string(), "glXSwapBuffers"));
    }

    candidates
        .into_iter()
        .filter_map(|(lib_path, func)| {
            let address = resolve_symbol(&lib_path, func)?;
            info!("found {func} in {lib_path} at 0x{address:x}");
            Some(HookTarget {
                address,
                label: lib_path,
            })
        })
        .collect()
}

/// Resolves the address of `func` exported by `lib_path` via `dlopen`/`dlsym`.
fn resolve_symbol(lib_path: &str, func: &str) -> Option<usize> {
    let c_lib = CString::new(lib_path).ok()?;
    let c_func = CString::new(func).ok()?;
    // SAFETY: dlopen/dlsym on a library the host process already maps.
    unsafe {
        let handle = libc::dlopen(c_lib.as_ptr(), libc::RTLD_LAZY);
        if handle.is_null() {
            return None;
        }
        let address = libc::dlsym(handle, c_func.as_ptr()) as usize;
        (address != 0).then_some(address)
    }
}

/// Finds the on-disk path of a loaded shared object by scanning the process
/// memory map.
fn find_library_path(partial_name: &str) -> Option<String> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open("/proc/self/maps").ok()?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // Format: address perms offset dev inode PATH
        if line.contains(partial_name) && line.contains(".so") {
            if let Some(path) = line.split_whitespace().last() {
                return Some(path.to_string());
            }
        }
    }
    None
}
