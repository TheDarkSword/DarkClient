//! Input interception.
//!
//! Two backends feed the same overlay logic: [`glfw`] (Minecraft ≤ 26.2)
//! swaps the window's callbacks for ours, [`sdl`] (26.3+) filters SDL's event
//! queue. Both report keys as GLFW key codes — the numbering `KeyboardKey`
//! and the saved keybinds use — so nothing above this module depends on the
//! windowing library.

mod glfw;
mod sdl;

use crate::state::{client, minecraft};
use log::info;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

// --- egui keyboard events --------------------------------------------------

/// Egui keyboard / text events accumulated by the input backend while the
/// overlay is open. Drained — and cleared — by `gather_egui_inputs` each
/// frame, so widgets like `TextEdit` and `DragValue` (used by the color
/// picker) receive characters and editing keys.
pub static PENDING_EVENTS: Mutex<Vec<egui::Event>> = Mutex::new(Vec::new());

fn push_event(event: egui::Event) {
    if let Ok(mut queue) = PENDING_EVENTS.lock() {
        queue.push(event);
    }
}

/// Maps a GLFW key code to its [`egui::Key`] counterpart, or `None` for keys
/// egui does not represent.
fn translate_glfw_key(key: i32) -> Option<egui::Key> {
    use egui::Key::*;
    Some(match key {
        32 => Space,
        39 => Quote,
        44 => Comma,
        45 => Minus,
        46 => Period,
        47 => Slash,
        48 => Num0,
        49 => Num1,
        50 => Num2,
        51 => Num3,
        52 => Num4,
        53 => Num5,
        54 => Num6,
        55 => Num7,
        56 => Num8,
        57 => Num9,
        59 => Semicolon,
        61 => Equals,
        65 => A,
        66 => B,
        67 => C,
        68 => D,
        69 => E,
        70 => F,
        71 => G,
        72 => H,
        73 => I,
        74 => J,
        75 => K,
        76 => L,
        77 => M,
        78 => N,
        79 => O,
        80 => P,
        81 => Q,
        82 => R,
        83 => S,
        84 => T,
        85 => U,
        86 => V,
        87 => W,
        88 => X,
        89 => Y,
        90 => Z,
        91 => OpenBracket,
        92 => Backslash,
        93 => CloseBracket,
        96 => Backtick,
        256 => Escape,
        257 => Enter,
        258 => Tab,
        259 => Backspace,
        260 => Insert,
        261 => Delete,
        262 => ArrowRight,
        263 => ArrowLeft,
        264 => ArrowDown,
        265 => ArrowUp,
        266 => PageUp,
        267 => PageDown,
        268 => Home,
        269 => End,
        // Numpad digits (GLFW_KEY_KP_0..9).
        320 => Num0,
        321 => Num1,
        322 => Num2,
        323 => Num3,
        324 => Num4,
        325 => Num5,
        326 => Num6,
        327 => Num7,
        328 => Num8,
        329 => Num9,
        330 => Period, // KP_DECIMAL
        331 => Slash,  // KP_DIVIDE
        333 => Minus,  // KP_SUBTRACT
        334 => Plus,   // KP_ADD
        335 => Enter,  // KP_ENTER
        336 => Equals, // KP_EQUAL
        290..=301 => match key - 289 {
            1 => F1,
            2 => F2,
            3 => F3,
            4 => F4,
            5 => F5,
            6 => F6,
            7 => F7,
            8 => F8,
            9 => F9,
            10 => F10,
            11 => F11,
            12 => F12,
            _ => return None,
        },
        _ => return None,
    })
}

// --- Public input state ----------------------------------------------------

/// Whether the overlay GUI is open. While open, input is consumed instead of
/// being forwarded to Minecraft.
pub static GUI_OPEN: AtomicBool = AtomicBool::new(false);

/// Last key pressed (GLFW key code), or `-1` once consumed. Used by the
/// keybind picker.
pub static LAST_KEY_PRESSED: AtomicI32 = AtomicI32::new(-1);

/// Snapshot of the mouse, updated by the input backend and read by the UI.
pub static MOUSE_STATE: Mutex<MouseState> = Mutex::new(MouseState::NEW);

#[derive(Default, Clone, Copy)]
pub struct MouseState {
    /// Cursor position in framebuffer pixels.
    pub x: f64,
    pub y: f64,
    pub left_down: bool,
    pub right_down: bool,
    pub left_clicked: bool,
    pub right_clicked: bool,
    /// Scroll-wheel delta accumulated since the last frame; drained — and
    /// reset — by the UI engine when it builds egui's input.
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl MouseState {
    const NEW: MouseState = MouseState {
        x: 0.0,
        y: 0.0,
        left_down: false,
        right_down: false,
        left_clicked: false,
        right_clicked: false,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
}

// --- Backend-agnostic event handling ---------------------------------------

const GLFW_KEY_RIGHT_SHIFT: i32 = 344;
const GLFW_KEY_ESCAPE: i32 = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseButton {
    Left,
    Right,
    Other,
}

/// Handles one key event, `key` being a GLFW key code (`None` for keys with
/// no GLFW equivalent). Returns `true` when the event must not reach
/// Minecraft.
fn handle_key(key: Option<i32>, action: KeyAction, modifiers: egui::Modifiers) -> bool {
    if let (Some(key), KeyAction::Press) = (key, action) {
        LAST_KEY_PRESSED.store(key, Ordering::Relaxed);
    }

    let gui_was_open = GUI_OPEN.load(Ordering::Relaxed);

    // Right Shift toggles the GUI; ESC also closes it while it is open.
    if action == KeyAction::Press
        && (key == Some(GLFW_KEY_RIGHT_SHIFT) || (key == Some(GLFW_KEY_ESCAPE) && gui_was_open))
    {
        toggle_gui();
    }

    // While the GUI is open, forward the keystroke to egui so text widgets
    // (DragValue editing, TextEdit, the color picker's hex / numeric fields)
    // see Backspace, arrows, Enter, digits, etc.
    if gui_was_open {
        if let Some(event) = clipboard_event(key, action, modifiers) {
            push_event(event);
        } else if let Some(egui_key) = key.and_then(translate_glfw_key) {
            push_event(egui::Event::Key {
                key: egui_key,
                physical_key: None,
                pressed: action != KeyAction::Release,
                repeat: action == KeyAction::Repeat,
                modifiers,
            });
        }
    }

    // Swallow the event whenever the GUI is open, or was open until this very
    // keystroke closed it (so ESC closing the GUI does not also reach
    // Minecraft and open its pause menu).
    if gui_was_open || GUI_OPEN.load(Ordering::Relaxed) {
        return true;
    }

    if let (Some(key), KeyAction::Press) = (key, action) {
        handle_module_keybind(key);
    }
    false
}

const GLFW_KEY_C: i32 = 67;
const GLFW_KEY_V: i32 = 86;
const GLFW_KEY_X: i32 = 88;

/// Ctrl+C / Ctrl+X / Ctrl+V as the clipboard events egui text fields expect.
/// The paste comes out empty: reading the clipboard from inside an input
/// callback could re-enter the windowing library's event loop, so
/// `gather_egui_inputs` fills it in on the render thread.
fn clipboard_event(
    key: Option<i32>,
    action: KeyAction,
    modifiers: egui::Modifiers,
) -> Option<egui::Event> {
    if action == KeyAction::Release || !modifiers.command {
        return None;
    }
    match key? {
        GLFW_KEY_C => Some(egui::Event::Copy),
        GLFW_KEY_X => Some(egui::Event::Cut),
        GLFW_KEY_V => Some(egui::Event::Paste(String::new())),
        _ => None,
    }
}

/// Handles typed text — already mapped through the keyboard layout, so this,
/// not [`handle_key`], supplies the characters egui text widgets insert.
/// Returns `true` when the event must not reach Minecraft.
fn handle_text(text: &str) -> bool {
    if GUI_OPEN.load(Ordering::Relaxed) {
        push_event(egui::Event::Text(text.to_owned()));
        return true;
    }
    false
}

/// Records a mouse button change. Returns `true` when the event must not
/// reach Minecraft.
fn handle_mouse_button(button: MouseButton, pressed: bool) -> bool {
    if let Ok(mut state) = MOUSE_STATE.lock() {
        match (button, pressed) {
            (MouseButton::Left, true) => {
                state.left_down = true;
                state.left_clicked = true;
            }
            (MouseButton::Left, false) => state.left_down = false,
            (MouseButton::Right, true) => {
                state.right_down = true;
                state.right_clicked = true;
            }
            (MouseButton::Right, false) => state.right_down = false,
            (MouseButton::Other, _) => {}
        }
    }
    GUI_OPEN.load(Ordering::Relaxed)
}

/// Records the cursor position, in framebuffer pixels.
fn handle_cursor_moved(x: f64, y: f64) {
    if let Ok(mut state) = MOUSE_STATE.lock() {
        state.x = x;
        state.y = y;
    }
}

/// While the GUI is open, feeds the wheel to egui and swallows it — otherwise
/// it would also reach Minecraft and change the selected hotbar slot. Returns
/// `true` when the event must not reach Minecraft.
fn handle_scroll(x: f32, y: f32) -> bool {
    if !GUI_OPEN.load(Ordering::Relaxed) {
        return false;
    }
    if let Ok(mut state) = MOUSE_STATE.lock() {
        state.scroll_x += x;
        state.scroll_y += y;
    }
    true
}

/// Toggles the overlay GUI and the matching cursor-capture mode.
fn toggle_gui() {
    // `fetch_xor` returns the previous value; the new state is its negation.
    let open = !GUI_OPEN.fetch_xor(true, Ordering::Relaxed);

    // Persist anything the user changed in the menu before it closes.
    if !open {
        crate::config::save();
    }

    match BACKEND.get() {
        Some(Backend::Glfw) if open => glfw::gui_opened(),
        Some(Backend::Glfw) => glfw::gui_closed(minecraft_screen_open()),
        Some(Backend::Sdl) if open => sdl::gui_opened(),
        Some(Backend::Sdl) => sdl::gui_closed(minecraft_screen_open()),
        None => {}
    }
}

/// Toggles any module whose keybind matches `key`, when in-world.
fn handle_module_keybind(key: i32) {
    let minecraft = minecraft();
    if !minecraft.current_screen_is_null() || !minecraft.in_world() {
        return;
    }

    let mut changed = false;
    for handle in client().modules.handles() {
        let Ok(mut module) = handle.lock() else {
            continue;
        };
        if module.get_module_data().key_bind as i32 != key {
            continue;
        }

        let enabled = !module.get_module_data().enabled;
        info!(
            "{} {}",
            module.get_module_data().name(),
            if enabled { "enabled" } else { "disabled" }
        );
        if enabled {
            let _ = module.on_start();
        } else {
            let _ = module.on_stop();
        }
        module.get_module_data_mut().set_enabled(enabled);
        changed = true;
    }

    // Persist the new enabled state.
    if changed {
        crate::config::save();
    }
}

/// Whether Minecraft has one of its own screens open — meaning the cursor
/// should be visible. Best-effort: a JNI failure resolves to "screen open",
/// so the safer, recoverable outcome (a visible cursor) wins.
fn minecraft_screen_open() -> bool {
    !minecraft().current_screen_is_null()
}

// --- Lifecycle -------------------------------------------------------------

#[derive(Clone, Copy)]
enum Backend {
    Glfw,
    Sdl,
}

/// The backend whose hooks are installed, set once on success.
static BACKEND: OnceLock<Backend> = OnceLock::new();

/// Installs the input hooks once the window exists. Cheap to call every
/// frame: it returns immediately after the first success and only retries
/// while the window is not yet available.
pub fn init() {
    if BACKEND.get().is_some() {
        return;
    }
    let installed = match crate::graphic::sdl::api() {
        Some(api) => sdl::install(api).then_some(Backend::Sdl),
        None => glfw::install().then_some(Backend::Glfw),
    };
    if let Some(backend) = installed {
        let _ = BACKEND.set(backend);
    }
}

/// Restores the host's original input handling and cursor capture. Safe to
/// call even if the hooks were never installed.
pub fn cleanup() {
    match BACKEND.get() {
        Some(Backend::Glfw) => glfw::cleanup(minecraft_screen_open()),
        Some(Backend::Sdl) => sdl::cleanup(minecraft_screen_open()),
        None => {}
    }
}
