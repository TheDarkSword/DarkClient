//! GLFW input backend (Minecraft ≤ 26.2): swaps the window's mouse / cursor /
//! key / char / scroll callbacks for ours and forwards whatever the overlay
//! does not consume.

use super::{KeyAction, MouseButton};
use libloading::Library;
use log::info;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const GLFW_RELEASE: i32 = 0;
const GLFW_PRESS: i32 = 1;
const GLFW_MOUSE_BUTTON_LEFT: i32 = 0;
const GLFW_MOUSE_BUTTON_RIGHT: i32 = 1;

const GLFW_CURSOR: i32 = 0x0003_3001;
const GLFW_CURSOR_NORMAL: i32 = 0x0003_4001;
const GLFW_CURSOR_DISABLED: i32 = 0x0003_4003;

type MouseButtonFun = extern "C" fn(*mut c_void, i32, i32, i32);
type CursorPosFun = extern "C" fn(*mut c_void, f64, f64);
type KeyFun = extern "C" fn(*mut c_void, i32, i32, i32, i32);
type ScrollFun = extern "C" fn(*mut c_void, f64, f64);
type CharFun = extern "C" fn(*mut c_void, u32);

type GetCurrentContext = extern "C" fn() -> *mut c_void;
type SetMouseButtonCallback = extern "C" fn(*mut c_void, MouseButtonFun) -> *mut c_void;
type SetCursorPosCallback = extern "C" fn(*mut c_void, CursorPosFun) -> *mut c_void;
type SetKeyCallback = extern "C" fn(*mut c_void, KeyFun) -> *mut c_void;
type SetScrollCallback = extern "C" fn(*mut c_void, ScrollFun) -> *mut c_void;
type SetCharCallback = extern "C" fn(*mut c_void, CharFun) -> *mut c_void;
type SetInputMode = extern "C" fn(*mut c_void, i32, i32);
type SetCursorPos = extern "C" fn(*mut c_void, f64, f64);

/// Everything captured when the GLFW callbacks were swapped: the library (kept
/// alive so the resolved symbols stay valid), the window, the host's original
/// callbacks, and the two functions needed to toggle cursor capture.
struct GlfwHooks {
    library: Library,
    window: *mut c_void,
    original_mouse_button: *mut c_void,
    original_cursor_pos: *mut c_void,
    original_key: *mut c_void,
    original_scroll: *mut c_void,
    original_char: *mut c_void,
    set_input_mode: SetInputMode,
    set_cursor_pos: SetCursorPos,
}

// The raw pointers are only ever touched on the render/GLFW thread; the struct
// is published through `OnceLock` purely for read-only access.
unsafe impl Send for GlfwHooks {}
unsafe impl Sync for GlfwHooks {}

static HOOKS: OnceLock<GlfwHooks> = OnceLock::new();

/// Last real cursor position, stored as `f64` bits. While the GUI is open the
/// cursor reported to Minecraft is frozen here, so the camera does not spin.
static CURSOR_LOCK_X: AtomicU64 = AtomicU64::new(0);
static CURSOR_LOCK_Y: AtomicU64 = AtomicU64::new(0);

fn cursor_lock() -> (f64, f64) {
    (
        f64::from_bits(CURSOR_LOCK_X.load(Ordering::Relaxed)),
        f64::from_bits(CURSOR_LOCK_Y.load(Ordering::Relaxed)),
    )
}

fn set_cursor_lock(x: f64, y: f64) {
    CURSOR_LOCK_X.store(x.to_bits(), Ordering::Relaxed);
    CURSOR_LOCK_Y.store(y.to_bits(), Ordering::Relaxed);
}

fn translate_mods(mods: i32) -> egui::Modifiers {
    let ctrl = mods & 0x2 != 0;
    egui::Modifiers {
        alt: mods & 0x4 != 0,
        ctrl,
        shift: mods & 0x1 != 0,
        mac_cmd: false,
        // On non-mac, egui treats `command` as `ctrl`.
        command: ctrl,
    }
}

// --- Callbacks -------------------------------------------------------------

extern "C" fn on_mouse_button(window: *mut c_void, button: i32, action: i32, mods: i32) {
    let which = match button {
        GLFW_MOUSE_BUTTON_LEFT => MouseButton::Left,
        GLFW_MOUSE_BUTTON_RIGHT => MouseButton::Right,
        _ => MouseButton::Other,
    };
    if super::handle_mouse_button(which, action != GLFW_RELEASE) {
        return;
    }
    if let Some(hooks) = HOOKS.get() {
        if !hooks.original_mouse_button.is_null() {
            let original: MouseButtonFun =
                unsafe { std::mem::transmute(hooks.original_mouse_button) };
            original(window, button, action, mods);
        }
    }
}

extern "C" fn on_cursor_pos(window: *mut c_void, x: f64, y: f64) {
    super::handle_cursor_moved(x, y);

    // While the GUI is open, feed Minecraft the frozen position so dragging the
    // overlay does not move the camera; otherwise track the real cursor.
    let (send_x, send_y) = if super::GUI_OPEN.load(Ordering::Relaxed) {
        cursor_lock()
    } else {
        set_cursor_lock(x, y);
        (x, y)
    };

    if let Some(hooks) = HOOKS.get() {
        if !hooks.original_cursor_pos.is_null() {
            let original: CursorPosFun = unsafe { std::mem::transmute(hooks.original_cursor_pos) };
            original(window, send_x, send_y);
        }
    }
}

extern "C" fn on_key(window: *mut c_void, key: i32, scancode: i32, action: i32, mods: i32) {
    let key_action = match action {
        GLFW_RELEASE => KeyAction::Release,
        GLFW_PRESS => KeyAction::Press,
        _ => KeyAction::Repeat,
    };
    if super::handle_key(Some(key), key_action, translate_mods(mods)) {
        return;
    }
    if let Some(hooks) = HOOKS.get() {
        if !hooks.original_key.is_null() {
            let original: KeyFun = unsafe { std::mem::transmute(hooks.original_key) };
            original(window, key, scancode, action, mods);
        }
    }
}

extern "C" fn on_char(window: *mut c_void, codepoint: u32) {
    if let Some(ch) = char::from_u32(codepoint) {
        if super::handle_text(ch.encode_utf8(&mut [0; 4])) {
            return;
        }
    }
    if let Some(hooks) = HOOKS.get() {
        if !hooks.original_char.is_null() {
            let original: CharFun = unsafe { std::mem::transmute(hooks.original_char) };
            original(window, codepoint);
        }
    }
}

extern "C" fn on_scroll(window: *mut c_void, x_offset: f64, y_offset: f64) {
    if super::handle_scroll(x_offset as f32, y_offset as f32) {
        return;
    }
    if let Some(hooks) = HOOKS.get() {
        if !hooks.original_scroll.is_null() {
            let original: ScrollFun = unsafe { std::mem::transmute(hooks.original_scroll) };
            original(window, x_offset, y_offset);
        }
    }
}

// --- GUI cursor handling ---------------------------------------------------

/// GUI opened: release the cursor.
pub(super) fn gui_opened() {
    let Some(hooks) = HOOKS.get().filter(|hooks| !hooks.window.is_null()) else {
        return;
    };
    (hooks.set_input_mode)(hooks.window, GLFW_CURSOR, GLFW_CURSOR_NORMAL);
}

/// GUI closed: leave the cursor free over an open Minecraft screen (chat,
/// inventory, menu, …); otherwise restore it where Minecraft last had it and
/// re-capture it — avoids a camera jump on the next mouse move.
pub(super) fn gui_closed(screen_open: bool) {
    let Some(hooks) = HOOKS.get().filter(|hooks| !hooks.window.is_null()) else {
        return;
    };
    if screen_open {
        (hooks.set_input_mode)(hooks.window, GLFW_CURSOR, GLFW_CURSOR_NORMAL);
    } else {
        let (lock_x, lock_y) = cursor_lock();
        (hooks.set_cursor_pos)(hooks.window, lock_x, lock_y);
        (hooks.set_input_mode)(hooks.window, GLFW_CURSOR, GLFW_CURSOR_DISABLED);
    }
}

// --- Lifecycle -------------------------------------------------------------

/// Resolves the GLFW symbols and swaps in our callbacks. Returns `false`
/// until the window is ready.
pub(super) fn install() -> bool {
    if HOOKS.get().is_some() {
        return true;
    }
    match install_hooks() {
        Some(hooks) => HOOKS.set(hooks).is_ok(),
        None => false,
    }
}

fn install_hooks() -> Option<GlfwHooks> {
    let library = crate::graphic::platform::open_glfw_library()?;

    unsafe {
        let get_context = *library
            .get::<GetCurrentContext>(b"glfwGetCurrentContext")
            .ok()?;
        let set_mouse_button = *library
            .get::<SetMouseButtonCallback>(b"glfwSetMouseButtonCallback")
            .ok()?;
        let set_cursor_pos_cb = *library
            .get::<SetCursorPosCallback>(b"glfwSetCursorPosCallback")
            .ok()?;
        let set_key = *library.get::<SetKeyCallback>(b"glfwSetKeyCallback").ok()?;
        let set_scroll = *library
            .get::<SetScrollCallback>(b"glfwSetScrollCallback")
            .ok()?;
        let set_char = *library
            .get::<SetCharCallback>(b"glfwSetCharCallback")
            .ok()?;
        let set_input_mode = *library.get::<SetInputMode>(b"glfwSetInputMode").ok()?;
        let set_cursor_pos = *library.get::<SetCursorPos>(b"glfwSetCursorPos").ok()?;

        let window = get_context();
        if window.is_null() {
            return None;
        }

        info!("GLFW window acquired; installing input callbacks.");

        let original_mouse_button = set_mouse_button(window, on_mouse_button);
        let original_cursor_pos = set_cursor_pos_cb(window, on_cursor_pos);
        let original_key = set_key(window, on_key);
        let original_scroll = set_scroll(window, on_scroll);
        let original_char = set_char(window, on_char);

        // If the GUI was toggled on before hooks existed, release the cursor.
        if super::GUI_OPEN.load(Ordering::Relaxed) {
            set_input_mode(window, GLFW_CURSOR, GLFW_CURSOR_NORMAL);
        }

        Some(GlfwHooks {
            library,
            window,
            original_mouse_button,
            original_cursor_pos,
            original_key,
            original_scroll,
            original_char,
            set_input_mode,
            set_cursor_pos,
        })
    }
}

/// Restores the host's original GLFW callbacks, and the cursor mode Minecraft
/// itself wants right now: visible while one of its screens is open, captured
/// in-world. Forcing it captured would hide the cursor after a Panic
/// triggered from inside a menu.
pub(super) fn cleanup(screen_open: bool) {
    let Some(hooks) = HOOKS.get().filter(|hooks| !hooks.window.is_null()) else {
        return;
    };

    type RestoreCallback = extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
    let restorations: [(&[u8], *mut c_void); 5] = [
        (b"glfwSetMouseButtonCallback", hooks.original_mouse_button),
        (b"glfwSetCursorPosCallback", hooks.original_cursor_pos),
        (b"glfwSetKeyCallback", hooks.original_key),
        (b"glfwSetScrollCallback", hooks.original_scroll),
        (b"glfwSetCharCallback", hooks.original_char),
    ];
    unsafe {
        for (name, original) in restorations {
            if let Ok(restore) = hooks.library.get::<RestoreCallback>(name) {
                restore(hooks.window, original);
            }
        }
    }

    let cursor_mode = if screen_open {
        GLFW_CURSOR_NORMAL
    } else {
        GLFW_CURSOR_DISABLED
    };
    (hooks.set_input_mode)(hooks.window, GLFW_CURSOR, cursor_mode);
    info!("GLFW input callbacks restored.");
}
