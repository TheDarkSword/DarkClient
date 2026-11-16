//! Frame hook.
//!
//! Intercepts the host's buffer-swap call so the overlay renders and the
//! client ticks exactly once per frame. Per-platform entry-point resolution
//! lives in [`crate::graphic::platform`]; under the Vulkan backend the frame
//! is driven by [`crate::graphic::vulkan`]'s present hook instead.

use crate::game::prelude::*;
use crate::graphic::platform;
use crate::{gl, state, RUNNING};
use ilhook::x64::{CallbackOption, HookFlags, HookPoint, HookType, Hooker, Registers};
use log::{info, warn};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

static LAST_TICK: AtomicI32 = AtomicI32::new(0);
static GL_LOADED: AtomicBool = AtomicBool::new(false);

/// Wrapper that lets an `ilhook` handle cross thread boundaries. Dropping the
/// inner `HookPoint` restores the patched bytes, i.e. removes the hook.
pub struct HookHandle {
    _hook: HookPoint,
}

unsafe impl Send for HookHandle {}
unsafe impl Sync for HookHandle {}

/// The single active buffer-swap hook. Re-injection replaces it: the old
/// `HookHandle` drops and unhooks itself. `None` means nothing is installed.
static GLOBAL_HOOK: OnceLock<Mutex<Option<HookHandle>>> = OnceLock::new();

fn global_hook() -> &'static Mutex<Option<HookHandle>> {
    GLOBAL_HOOK.get_or_init(|| Mutex::new(None))
}

// --- Per-frame logic -------------------------------------------------------

/// The player's tick counter as last observed. Changes exactly when a new
/// game tick has run, so per-tick work can key off it.
pub fn last_tick() -> i32 {
    LAST_TICK.load(Ordering::Relaxed)
}

/// `ilhook` trampoline for the host's buffer-swap function.
unsafe extern "win64" fn swap_buffers_hook(_registers: *mut Registers, _user_data: usize) {
    on_frame();
}

/// Runs once per graphical frame, from the buffer-swap hook.
unsafe fn on_frame() {
    // If `cleanup_client()` has run, `RUNNING` is false — bail immediately:
    // no tick, no draw, the hooks are about to be torn down.
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }

    // Resolve the GL function pointers once, on the live render context.
    if !GL_LOADED.load(Ordering::Relaxed) {
        gl::load_with(platform::gl_proc_address);
        GL_LOADED.store(true, Ordering::Relaxed);
    }

    frame_logic();
    crate::graphic::ui_engine::render_gl();
}

/// Per-frame client logic, shared by every graphics backend.
pub(crate) fn frame_logic() {
    check_tick();
    // Ease combat-module rotations toward their target every frame — this is
    // what makes aiming smooth rather than the visible 20 Hz tick steps.
    crate::module::combat::rotation::update();
    // Install the input hooks lazily — they need the live window. The
    // overlay renders whether or not a world is loaded, so the GUI works
    // even when the client is injected from the main menu.
    crate::graphic::input::init();
}

/// Detects a new game tick by watching the player's tick counter, and ticks
/// every enabled module when one is observed.
fn check_tick() {
    // Attach this render thread to the JVM so it can make JNI calls.
    if state::env().is_err() {
        return;
    }

    // Modules run only in a world with a *living* player. Notably they stand
    // down while the player is dead on the respawn screen — a cheat still
    // acting then is an instant ban on most servers — and reactivate once the
    // player respawns. `set_active` starts/stops them on each transition.
    let player = living_player().unwrap_or_else(|e| {
        crate::error::warn_changed("frame: reading the player failed", &e);
        None
    });
    state::client().modules.set_active(player.is_some());
    let Some(player) = player else {
        return;
    };

    let tick_count = match player.tick_count() {
        Ok(count) => count,
        Err(e) => {
            crate::error::warn_changed("frame: reading the tick count failed", &e);
            return;
        }
    };

    // Any change is a new tick: a respawned or re-joined player is a new
    // `LocalPlayer` whose counter restarts from zero.
    if tick_count != LAST_TICK.load(Ordering::Relaxed) {
        LAST_TICK.store(tick_count, Ordering::Relaxed);
        // Keep our Netty handler on the live connection's pipeline.
        crate::net::ensure_installed();
        state::client().modules.tick();
    }
}

/// The player, when in a world and alive.
fn living_player() -> Result<Option<LocalPlayer>> {
    let mc = state::minecraft();
    if mc.level()?.is_none() {
        return Ok(None);
    }
    let Some(player) = mc.player()? else {
        return Ok(None);
    };
    Ok(player.is_alive()?.then_some(player))
}

// --- Hook installation -----------------------------------------------------

/// Stores a freshly built hook. Idempotent across re-injection: the previous
/// `HookHandle` drops here, restoring its patched bytes.
fn store_hook(hook: HookPoint, label: &str) {
    let mut guard = global_hook().lock().unwrap();
    if guard.is_some() {
        info!("Replacing the existing buffer-swap hook.");
    }
    *guard = Some(HookHandle { _hook: hook });
    info!(">>> HOOK ACTIVE ON: {} <<<", label);
}

/// Builds an `ilhook` `Hooker` for `target_addr` routed to [`swap_buffers_hook`].
fn hooker_for(target_addr: usize) -> Hooker {
    Hooker::new(
        target_addr,
        HookType::JmpBack(swap_buffers_hook),
        CallbackOption::None,
        0,
        HookFlags::empty(),
    )
}

/// Installs the hook that drives the overlay: the Vulkan present hook when
/// the game renders through Vulkan, otherwise the buffer-swap hook, trying
/// every platform-provided target until one hooks successfully.
pub fn install_hooks() -> Result<()> {
    if crate::graphic::vulkan::install()? {
        return Ok(());
    }
    for target in platform::frame_hook_targets() {
        // SAFETY: `target.address` is a function address resolved by the
        // platform layer; `ilhook` patches it in place.
        let result = unsafe { hooker_for(target.address).hook() };
        match result {
            Ok(hook) => {
                store_hook(hook, &target.label);
                return Ok(());
            }
            Err(e) => warn!("hook install failed on {}: {e:?}", target.label),
        }
    }
    Err(Error::NoFrameHook)
}

/// Removes the active frame hook, restoring the original bytes or pointer.
pub fn uninstall_hooks() {
    crate::graphic::vulkan::uninstall();
    let mut guard = global_hook().lock().unwrap();
    if guard.take().is_some() {
        info!("Buffer-swap hook removed; memory restored.");
    } else {
        info!("No active hook to remove.");
    }
}
