//! SDL3 input backend (Minecraft 26.3+). SDL has no per-window callbacks to
//! swap: instead an event filter sits in front of SDL's queue, sees every
//! input event before Minecraft polls it, and drops what the overlay
//! consumes. A filter Minecraft (or a mod) installed earlier is chained.

use super::{KeyAction, MouseButton};
use crate::graphic::sdl::{EventFilter, SdlApi};
use log::{info, warn};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

const SDL_EVENT_KEY_DOWN: u32 = 0x300;
const SDL_EVENT_KEY_UP: u32 = 0x301;
const SDL_EVENT_TEXT_INPUT: u32 = 0x303;
const SDL_EVENT_MOUSE_MOTION: u32 = 0x400;
const SDL_EVENT_MOUSE_BUTTON_DOWN: u32 = 0x401;
const SDL_EVENT_MOUSE_BUTTON_UP: u32 = 0x402;
const SDL_EVENT_MOUSE_WHEEL: u32 = 0x403;

const SDL_BUTTON_LEFT: u8 = 1;
const SDL_BUTTON_RIGHT: u8 = 3;
const SDL_MOUSEWHEEL_FLIPPED: u32 = 1;

const SDL_KMOD_SHIFT: u16 = 0x0003;
const SDL_KMOD_CTRL: u16 = 0x00C0;
const SDL_KMOD_ALT: u16 = 0x0300;

// Event layouts from SDL_events.h. Every field is spelled out so the offsets
// match the C structs; the asserts below pin them.

#[repr(C)]
struct KeyboardEvent {
    kind: u32,
    reserved: u32,
    timestamp: u64,
    window_id: u32,
    which: u32,
    scancode: u32,
    key: u32,
    modifiers: u16,
    raw: u16,
    down: bool,
    repeat: bool,
}

#[repr(C)]
struct TextInputEvent {
    kind: u32,
    reserved: u32,
    timestamp: u64,
    window_id: u32,
    text: *const c_char,
}

#[repr(C)]
struct MouseMotionEvent {
    kind: u32,
    reserved: u32,
    timestamp: u64,
    window_id: u32,
    which: u32,
    state: u32,
    x: f32,
    y: f32,
    xrel: f32,
    yrel: f32,
}

#[repr(C)]
struct MouseButtonEvent {
    kind: u32,
    reserved: u32,
    timestamp: u64,
    window_id: u32,
    which: u32,
    button: u8,
    down: bool,
    clicks: u8,
    padding: u8,
    x: f32,
    y: f32,
}

#[repr(C)]
struct MouseWheelEvent {
    kind: u32,
    reserved: u32,
    timestamp: u64,
    window_id: u32,
    which: u32,
    x: f32,
    y: f32,
    direction: u32,
}

const _: () = {
    use std::mem::offset_of;
    assert!(offset_of!(KeyboardEvent, scancode) == 24);
    assert!(offset_of!(KeyboardEvent, modifiers) == 32);
    assert!(offset_of!(KeyboardEvent, down) == 36);
    assert!(offset_of!(KeyboardEvent, repeat) == 37);
    assert!(offset_of!(TextInputEvent, text) == 24);
    assert!(offset_of!(MouseMotionEvent, x) == 28);
    assert!(offset_of!(MouseButtonEvent, button) == 24);
    assert!(offset_of!(MouseButtonEvent, down) == 25);
    assert!(offset_of!(MouseWheelEvent, x) == 24);
    assert!(offset_of!(MouseWheelEvent, direction) == 32);
};

struct SdlHooks {
    api: &'static SdlApi,
    /// `SDL_Window*` — what `Window.handle()` holds on SDL builds.
    window: *mut c_void,
    previous_filter: Option<EventFilter>,
    previous_userdata: *mut c_void,
}

// The window and userdata pointers are opaque handles only passed back to SDL.
unsafe impl Send for SdlHooks {}
unsafe impl Sync for SdlHooks {}

static HOOKS: OnceLock<SdlHooks> = OnceLock::new();

/// Whether we turned text input on (Minecraft only enables it while one of its
/// own text fields is focused), so we know to turn it back off.
static TEXT_INPUT_STARTED: AtomicBool = AtomicBool::new(false);

/// Maps an SDL scancode to the GLFW key code at the same physical position.
fn scancode_to_glfw(scancode: u32) -> Option<i32> {
    let sc = scancode as i32;
    Some(match sc {
        4..=29 => 65 + sc - 4,       // A..Z
        30..=38 => 49 + sc - 30,     // 1..9
        39 => 48,                    // 0
        40 => 257,                   // Return
        41 => 256,                   // Escape
        42 => 259,                   // Backspace
        43 => 258,                   // Tab
        44 => 32,                    // Space
        45 => 45,                    // Minus
        46 => 61,                    // Equals
        47 => 91,                    // Left bracket
        48 => 93,                    // Right bracket
        49 | 50 => 92,               // Backslash, ISO hash
        51 => 59,                    // Semicolon
        52 => 39,                    // Apostrophe
        53 => 96,                    // Grave
        54 => 44,                    // Comma
        55 => 46,                    // Period
        56 => 47,                    // Slash
        57 => 280,                   // Caps lock
        58..=69 => 290 + sc - 58,    // F1..F12
        70 => 283,                   // Print screen
        71 => 281,                   // Scroll lock
        72 => 284,                   // Pause
        73 => 260,                   // Insert
        74 => 268,                   // Home
        75 => 266,                   // Page up
        76 => 261,                   // Delete
        77 => 269,                   // End
        78 => 267,                   // Page down
        79 => 262,                   // Right
        80 => 263,                   // Left
        81 => 264,                   // Down
        82 => 265,                   // Up
        83 => 282,                   // Num lock
        84 => 331,                   // KP divide
        85 => 332,                   // KP multiply
        86 => 333,                   // KP minus
        87 => 334,                   // KP plus
        88 => 335,                   // KP enter
        89..=97 => 321 + sc - 89,    // KP 1..9
        98 => 320,                   // KP 0
        99 => 330,                   // KP period
        100 => 162,                  // ISO backslash (GLFW "world 2")
        101 => 348,                  // Menu
        103 => 336,                  // KP equals
        104..=115 => 302 + sc - 104, // F13..F24
        224 => 341,                  // Left ctrl
        225 => 340,                  // Left shift
        226 => 342,                  // Left alt
        227 => 343,                  // Left super
        228 => 345,                  // Right ctrl
        229 => 344,                  // Right shift
        230 => 346,                  // Right alt
        231 => 347,                  // Right super
        _ => return None,
    })
}

fn translate_mods(mods: u16) -> egui::Modifiers {
    let ctrl = mods & SDL_KMOD_CTRL != 0;
    egui::Modifiers {
        alt: mods & SDL_KMOD_ALT != 0,
        ctrl,
        shift: mods & SDL_KMOD_SHIFT != 0,
        mac_cmd: false,
        command: ctrl,
    }
}

/// Window coordinates → framebuffer pixels, the unit the overlay works in.
fn pixel_density(hooks: &SdlHooks) -> f64 {
    // SAFETY: SDL validates the window pointer and returns 0 on failure.
    let density = unsafe { (hooks.api.get_window_pixel_density)(hooks.window) };
    if density > 0.0 {
        density as f64
    } else {
        1.0
    }
}

unsafe extern "C" fn event_filter(_userdata: *mut c_void, event: *mut c_void) -> bool {
    let Some(hooks) = HOOKS.get() else {
        return true;
    };
    if event.is_null() {
        return true;
    }
    if consume(hooks, event) {
        return false;
    }
    match hooks.previous_filter {
        Some(previous) => previous(hooks.previous_userdata, event),
        None => true,
    }
}

/// Runs the overlay's handling of one event; `true` drops it.
///
/// # Safety
/// `event` must point to a live `SDL_Event`.
unsafe fn consume(hooks: &SdlHooks, event: *mut c_void) -> bool {
    match *(event as *const u32) {
        SDL_EVENT_KEY_DOWN | SDL_EVENT_KEY_UP => {
            let event = &*(event as *const KeyboardEvent);
            let action = match (event.down, event.repeat) {
                (false, _) => KeyAction::Release,
                (true, true) => KeyAction::Repeat,
                (true, false) => KeyAction::Press,
            };
            super::handle_key(
                scancode_to_glfw(event.scancode),
                action,
                translate_mods(event.modifiers),
            )
        }
        SDL_EVENT_TEXT_INPUT => {
            let event = &*(event as *const TextInputEvent);
            if event.text.is_null() {
                return super::GUI_OPEN.load(Ordering::Relaxed);
            }
            super::handle_text(&CStr::from_ptr(event.text).to_string_lossy())
        }
        SDL_EVENT_MOUSE_MOTION => {
            let event = &*(event as *const MouseMotionEvent);
            let density = pixel_density(hooks);
            super::handle_cursor_moved(event.x as f64 * density, event.y as f64 * density);
            super::GUI_OPEN.load(Ordering::Relaxed)
        }
        SDL_EVENT_MOUSE_BUTTON_DOWN | SDL_EVENT_MOUSE_BUTTON_UP => {
            let event = &*(event as *const MouseButtonEvent);
            let button = match event.button {
                SDL_BUTTON_LEFT => MouseButton::Left,
                SDL_BUTTON_RIGHT => MouseButton::Right,
                _ => MouseButton::Other,
            };
            super::handle_mouse_button(button, event.down)
        }
        SDL_EVENT_MOUSE_WHEEL => {
            let event = &*(event as *const MouseWheelEvent);
            let sign = if event.direction == SDL_MOUSEWHEEL_FLIPPED {
                -1.0
            } else {
                1.0
            };
            super::handle_scroll(event.x * sign, event.y * sign)
        }
        _ => false,
    }
}

// --- GUI cursor handling ---------------------------------------------------

/// GUI opened: free the cursor, enable text input for egui's fields, and
/// seed the cursor position (no motion event arrives until the mouse moves).
pub(super) fn gui_opened() {
    let Some(hooks) = HOOKS.get() else {
        return;
    };
    let api = hooks.api;
    // SAFETY: SDL validates the window pointer.
    unsafe {
        (api.set_window_relative_mouse_mode)(hooks.window, false);
        if !(api.text_input_active)(hooks.window) && (api.start_text_input)(hooks.window) {
            TEXT_INPUT_STARTED.store(true, Ordering::Relaxed);
        }
        let (mut x, mut y) = (0.0_f32, 0.0_f32);
        (api.get_mouse_state)(&mut x, &mut y);
        let density = pixel_density(hooks);
        super::handle_cursor_moved(x as f64 * density, y as f64 * density);
    }
}

/// GUI closed: give text input back, and re-capture the mouse unless a
/// Minecraft screen (chat, inventory, menu, …) still needs the cursor.
pub(super) fn gui_closed(screen_open: bool) {
    let Some(hooks) = HOOKS.get() else {
        return;
    };
    // SAFETY: SDL validates the window pointer.
    unsafe {
        if TEXT_INPUT_STARTED.swap(false, Ordering::Relaxed) {
            (hooks.api.stop_text_input)(hooks.window);
        }
        (hooks.api.set_window_relative_mouse_mode)(hooks.window, !screen_open);
    }
}

// --- Lifecycle -------------------------------------------------------------

/// Installs the event filter. Returns `false` until the window exists.
pub(super) fn install(api: &'static SdlApi) -> bool {
    if HOOKS.get().is_some() {
        return true;
    }
    let window = match crate::state::window().handle() {
        Ok(handle) if handle != 0 => handle as *mut c_void,
        Ok(_) => return false,
        Err(e) => {
            warn!("SDL window handle unavailable: {e}");
            return false;
        }
    };

    let mut previous_filter: Option<EventFilter> = None;
    let mut previous_userdata: *mut c_void = std::ptr::null_mut();
    // SAFETY: both out-pointers are valid for writes.
    unsafe { (api.get_event_filter)(&mut previous_filter, &mut previous_userdata) };

    // Published before the filter goes live: SDL runs the new filter over the
    // already-queued events from inside `SDL_SetEventFilter`.
    let hooks = SdlHooks {
        api,
        window,
        previous_filter,
        previous_userdata,
    };
    if HOOKS.set(hooks).is_err() {
        return true;
    }
    // SAFETY: `event_filter` matches `SDL_EventFilter`.
    unsafe { (api.set_event_filter)(Some(event_filter), std::ptr::null_mut()) };
    info!(
        "SDL window acquired; event filter installed (previous filter: {}).",
        previous_filter.is_some()
    );

    if super::GUI_OPEN.load(Ordering::Relaxed) {
        gui_opened();
    }
    true
}

/// Puts the previous event filter back and, if the GUI is open, hands the
/// cursor and text input back to the state Minecraft expects.
pub(super) fn cleanup(screen_open: bool) {
    let Some(hooks) = HOOKS.get() else {
        return;
    };
    // SAFETY: restoring exactly what `SDL_GetEventFilter` returned.
    unsafe { (hooks.api.set_event_filter)(hooks.previous_filter, hooks.previous_userdata) };
    if super::GUI_OPEN.load(Ordering::Relaxed) {
        gui_closed(screen_open);
    }
    info!("SDL event filter removed.");
}

#[cfg(test)]
mod tests {
    use super::scancode_to_glfw;

    #[test]
    fn letters_digits_and_controls_map_to_glfw() {
        assert_eq!(scancode_to_glfw(4), Some(65)); // A
        assert_eq!(scancode_to_glfw(29), Some(90)); // Z
        assert_eq!(scancode_to_glfw(30), Some(49)); // 1
        assert_eq!(scancode_to_glfw(39), Some(48)); // 0
        assert_eq!(scancode_to_glfw(41), Some(256)); // Escape
        assert_eq!(scancode_to_glfw(229), Some(344)); // Right shift
    }

    #[test]
    fn function_and_keypad_ranges_map_to_glfw() {
        assert_eq!(scancode_to_glfw(58), Some(290)); // F1
        assert_eq!(scancode_to_glfw(69), Some(301)); // F12
        assert_eq!(scancode_to_glfw(115), Some(313)); // F24
        assert_eq!(scancode_to_glfw(89), Some(321)); // KP 1
        assert_eq!(scancode_to_glfw(98), Some(320)); // KP 0
    }

    #[test]
    fn unknown_scancodes_have_no_glfw_key() {
        assert_eq!(scancode_to_glfw(0), None);
        assert_eq!(scancode_to_glfw(102), None);
        assert_eq!(scancode_to_glfw(500), None);
    }
}
