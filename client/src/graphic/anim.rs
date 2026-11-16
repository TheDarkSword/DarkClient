//! Frame-rate-independent animation toolkit.
//!
//! egui's built-in `animate_*` helpers advance on egui's own clock. This
//! module animates against the **real** elapsed time fed into `RawInput`
//! (see [`ui_engine`](crate::graphic::ui_engine)), so motion stays correct no
//! matter how the host paces frames — including while Minecraft is paused.
//!
//! All state lives in one global store keyed by [`egui::Id`], so adding an
//! animation is a single call — nothing to declare, own or thread through:
//!
//! ```ignore
//! let open  = anim::toggle(ctx, Id::new("menu"), is_open, 0.18, Easing::InOut);
//! let pos   = anim::spring_pos(ctx, Id::new("panel"), target, SpringCfg::PANEL);
//! let value = anim::ease_to(ctx, Id::new("bar"), target, 0.2, Easing::Out);
//! ```
//!
//! Entries are reclaimed automatically: call [`gc`] once per frame.

use egui::{Context, Id, Pos2, Vec2};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Largest physics step integrated in a single frame. Caps spring blow-up
/// when the host stalls: a long hitch is absorbed as one ~15 FPS step.
const MAX_DT: f32 = 1.0 / 15.0;

/// Spring integration substeps per frame — keeps stiff springs stable.
const SUBSTEPS: u32 = 4;

/// Entries untouched for this many seconds are dropped by [`gc`].
const STALE_AFTER: f64 = 3.0;

// --- Easing ----------------------------------------------------------------

/// Easing curve applied to tweened progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    In,
    Out,
    InOut,
}

impl Easing {
    /// Maps linear progress `t` (`0..=1`) through the curve.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::In => t * t * t,
            Easing::Out => 1.0 - (1.0 - t).powi(3),
            Easing::InOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

// --- Spring tuning ---------------------------------------------------------

/// Tuning for a damped spring.
#[derive(Clone, Copy, Debug)]
pub struct SpringCfg {
    pub stiffness: f32,
    pub damping: f32,
}

impl SpringCfg {
    /// Smooth, weighty motion — draggable panels.
    pub const PANEL: SpringCfg = SpringCfg {
        stiffness: 320.0,
        damping: 24.0,
    };
}

// --- Global store ----------------------------------------------------------

struct Tween {
    from: f32,
    target: f32,
    duration: f32,
    easing: Easing,
    start: f64,
    last_seen: f64,
}

struct SpringState {
    value: Vec2,
    velocity: Vec2,
    last_seen: f64,
}

#[derive(Default)]
struct Store {
    tweens: HashMap<Id, Tween>,
    springs: HashMap<Id, SpringState>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

/// Real frame time (seconds) and the clamped frame delta.
fn clock(ctx: &Context) -> (f64, f32) {
    ctx.input(|i| (i.time, i.stable_dt.clamp(0.0, MAX_DT)))
}

// --- Tweens ----------------------------------------------------------------

/// Eases a value toward `target` over `duration` seconds.
///
/// Re-targeting mid-flight re-anchors from the currently displayed value, so
/// interrupted animations never jump.
pub fn ease_to(ctx: &Context, id: Id, target: f32, duration: f32, easing: Easing) -> f32 {
    let (now, _) = clock(ctx);
    let mut store = store().lock().unwrap();
    let tween = store.tweens.entry(id).or_insert(Tween {
        from: target,
        target,
        duration,
        easing,
        start: now,
        last_seen: now,
    });
    tween.last_seen = now;

    if (tween.target - target).abs() > f32::EPSILON {
        tween.from = sample(tween, now);
        tween.target = target;
        tween.duration = duration;
        tween.easing = easing;
        tween.start = now;
    }
    sample(tween, now)
}

/// Animates a boolean into a `0..1` factor: eases to `1.0` while `on`.
pub fn toggle(ctx: &Context, id: Id, on: bool, duration: f32, easing: Easing) -> f32 {
    ease_to(ctx, id, if on { 1.0 } else { 0.0 }, duration, easing)
}

fn sample(t: &Tween, now: f64) -> f32 {
    if t.duration <= 0.0 {
        return t.target;
    }
    let progress = ((now - t.start) as f32 / t.duration).clamp(0.0, 1.0);
    t.from + (t.target - t.from) * t.easing.apply(progress)
}

// --- Springs ---------------------------------------------------------------

/// Spring-smooths a position toward `target`.
pub fn spring_pos(ctx: &Context, id: Id, target: Pos2, cfg: SpringCfg) -> Pos2 {
    spring_vec2(ctx, id, target.to_vec2(), cfg).to_pos2()
}

/// Spring-smooths a vector toward `target` with a sub-stepped integrator.
pub fn spring_vec2(ctx: &Context, id: Id, target: Vec2, cfg: SpringCfg) -> Vec2 {
    let (now, dt) = clock(ctx);
    let mut store = store().lock().unwrap();
    let spring = store.springs.entry(id).or_insert(SpringState {
        value: target,
        velocity: Vec2::ZERO,
        last_seen: now,
    });
    spring.last_seen = now;

    let sub_dt = dt / SUBSTEPS as f32;
    for _ in 0..SUBSTEPS {
        let displacement = spring.value - target;
        let accel = -cfg.stiffness * displacement - cfg.damping * spring.velocity;
        spring.velocity += accel * sub_dt;
        spring.value += spring.velocity * sub_dt;
    }
    spring.value
}

// --- Maintenance -----------------------------------------------------------

/// Drops animation state that has not been touched recently. Call once per
/// frame so transient ids (e.g. per-notification) cannot accumulate.
pub fn gc(ctx: &Context) {
    let now = ctx.input(|i| i.time);
    let mut store = store().lock().unwrap();
    store.tweens.retain(|_, t| now - t.last_seen < STALE_AFTER);
    store.springs.retain(|_, s| now - s.last_seen < STALE_AFTER);
}
