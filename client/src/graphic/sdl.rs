//! SDL3 entry points.
//!
//! Minecraft 26.3+ replaced GLFW with SDL3 for windowing and input. [`api`]
//! resolves the handful of SDL functions the overlay needs from the library
//! the game already loaded; it is `None` on older versions, which keep using
//! GLFW.

use std::ffi::{c_char, c_void, CString};
use std::sync::OnceLock;

use libloading::Library;
use log::info;

/// `SDL_EventFilter`: returns `false` to drop the event.
pub type EventFilter = unsafe extern "C" fn(userdata: *mut c_void, event: *mut c_void) -> bool;

pub struct SdlApi {
    _library: Library,
    gl_get_proc_address: unsafe extern "C" fn(*const c_char) -> *const c_void,
    pub set_event_filter: unsafe extern "C" fn(Option<EventFilter>, *mut c_void),
    pub get_event_filter: unsafe extern "C" fn(*mut Option<EventFilter>, *mut *mut c_void) -> bool,
    pub set_window_relative_mouse_mode: unsafe extern "C" fn(*mut c_void, bool) -> bool,
    pub start_text_input: unsafe extern "C" fn(*mut c_void) -> bool,
    pub stop_text_input: unsafe extern "C" fn(*mut c_void) -> bool,
    pub text_input_active: unsafe extern "C" fn(*mut c_void) -> bool,
    pub get_mouse_state: unsafe extern "C" fn(*mut f32, *mut f32) -> u32,
    pub get_window_pixel_density: unsafe extern "C" fn(*mut c_void) -> f32,
}

impl SdlApi {
    fn load(library: Library) -> Option<SdlApi> {
        // SAFETY: every symbol is resolved with the exact C signature from
        // the SDL 3 headers.
        unsafe {
            Some(SdlApi {
                gl_get_proc_address: *library.get(b"SDL_GL_GetProcAddress").ok()?,
                set_event_filter: *library.get(b"SDL_SetEventFilter").ok()?,
                get_event_filter: *library.get(b"SDL_GetEventFilter").ok()?,
                set_window_relative_mouse_mode: *library
                    .get(b"SDL_SetWindowRelativeMouseMode")
                    .ok()?,
                start_text_input: *library.get(b"SDL_StartTextInput").ok()?,
                stop_text_input: *library.get(b"SDL_StopTextInput").ok()?,
                text_input_active: *library.get(b"SDL_TextInputActive").ok()?,
                get_mouse_state: *library.get(b"SDL_GetMouseState").ok()?,
                get_window_pixel_density: *library.get(b"SDL_GetWindowPixelDensity").ok()?,
                _library: library,
            })
        }
    }

    /// Resolves an OpenGL function through SDL, which knows whether the
    /// context is GLX, EGL or WGL. Null when SDL has no GL library loaded
    /// (e.g. the Vulkan backend).
    pub fn gl_proc_address(&self, name: &str) -> *const c_void {
        let Ok(name) = CString::new(name) else {
            return std::ptr::null();
        };
        // SAFETY: `name` is a valid NUL-terminated string.
        unsafe { (self.gl_get_proc_address)(name.as_ptr()) }
    }
}

/// The SDL3 API, if the game runs on SDL3.
pub fn api() -> Option<&'static SdlApi> {
    static API: OnceLock<Option<SdlApi>> = OnceLock::new();
    API.get_or_init(|| {
        let api = crate::graphic::platform::open_sdl_library().and_then(SdlApi::load);
        if api.is_some() {
            info!("SDL3 detected; using the SDL windowing backend");
        }
        api
    })
    .as_ref()
}
