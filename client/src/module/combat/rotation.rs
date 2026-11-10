//! Shared rotation system for the combat modules.
//!
//! Combat modules pick a target rotation each game tick (~20 Hz); the global
//! [`RotationController`] then eases the player's camera toward it **every
//! rendered frame** (see [`update`], driven from the frame hook). Per-frame
//! exponential smoothing — frame-rate independent — is what makes the motion
//! fluid, instead of the visible 20 Hz jumps of a per-tick approach.

use crate::game::prelude::*;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A target is dropped if no module refreshes it within this long — so a
/// module releases the camera simply by no longer calling [`aim`].
const TARGET_TIMEOUT: Duration = Duration::from_millis(150);

/// Frame time is clamped to this, so a render hitch cannot snap the camera.
const MAX_FRAME_DT: f32 = 0.1;

/// A yaw/pitch pair, in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub const fn new(yaw: f32, pitch: f32) -> Rotation {
        Rotation { yaw, pitch }
    }

    /// The rotation that looks from `from` to `to`.
    pub fn towards(from: Vec3, to: Vec3) -> Rotation {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        let ground = (dx * dx + dz * dz).sqrt();
        Rotation {
            yaw: wrap_degrees((dz.atan2(dx).to_degrees() - 90.0) as f32),
            pitch: (-dy.atan2(ground).to_degrees() as f32).clamp(-90.0, 90.0),
        }
    }

    /// Total angular distance to `other`, in degrees.
    pub fn angle_to(self, other: Rotation) -> f32 {
        let dyaw = wrap_degrees(other.yaw - self.yaw);
        let dpitch = other.pitch - self.pitch;
        (dyaw * dyaw + dpitch * dpitch).sqrt()
    }

    /// Moves a fraction `alpha` of the way toward `target`, taking the shortest
    /// way around for the yaw.
    fn lerp_towards(self, target: Rotation, alpha: f32) -> Rotation {
        let dyaw = wrap_degrees(target.yaw - self.yaw);
        let dpitch = target.pitch - self.pitch;
        Rotation {
            yaw: self.yaw + dyaw * alpha,
            pitch: (self.pitch + dpitch * alpha).clamp(-90.0, 90.0),
        }
    }
}

/// Normalizes an angle to `[-180, 180)`.
pub fn wrap_degrees(mut angle: f32) -> f32 {
    angle %= 360.0;
    if angle >= 180.0 {
        angle -= 360.0;
    } else if angle < -180.0 {
        angle += 360.0;
    }
    angle
}

/// What the camera is being eased toward.
#[derive(Debug, Clone, Copy)]
struct Target {
    rotation: Rotation,
    /// Easing rate — larger converges faster (see [`ease_camera`]).
    speed: f32,
    /// When a module last refreshed this target.
    refreshed: Instant,
}

/// Eases the player's camera toward a target rotation, frame by frame.
struct RotationController {
    target: Option<Target>,
    last_frame: Option<Instant>,
}

impl RotationController {
    const fn new() -> RotationController {
        RotationController {
            target: None,
            last_frame: None,
        }
    }
}

static CONTROLLER: Mutex<RotationController> = Mutex::new(RotationController::new());

/// Aims the camera toward `rotation`, easing at `speed`. A combat module calls
/// this every tick while it has a target; it releases the camera simply by
/// stopping (the target expires after [`TARGET_TIMEOUT`]).
pub fn aim(rotation: Rotation, speed: f32) {
    if let Ok(mut controller) = CONTROLLER.lock() {
        controller.target = Some(Target {
            rotation,
            speed,
            refreshed: Instant::now(),
        });
    }
}

/// Advances the camera one frame toward the active target. Driven from the
/// frame hook; does nothing — and touches no JNI — while no target is set.
pub fn update() {
    let step = {
        let Ok(mut controller) = CONTROLLER.lock() else {
            return;
        };
        let now = Instant::now();
        let dt = controller
            .last_frame
            .map_or(0.0, |last| (now - last).as_secs_f32())
            .min(MAX_FRAME_DT);
        controller.last_frame = Some(now);

        match controller.target {
            Some(target) if target.refreshed.elapsed() <= TARGET_TIMEOUT => Some((target, dt)),
            _ => {
                controller.target = None;
                None
            }
        }
    };

    let Some((target, dt)) = step else {
        return;
    };
    if dt <= 0.0 {
        return;
    }
    // A failed JNI call here must never abort the frame.
    let _ = ease_camera(target, dt);
}

/// Reads the player's current rotation, eases it one step toward `target`, and
/// writes it back.
fn ease_camera(target: Target, dt: f32) -> anyhow::Result<()> {
    let Some(player) = minecraft().player()? else {
        return Ok(());
    };
    let current = Rotation::new(player.get_y_rot()?, player.get_x_rot()?);
    // Exponential smoothing — frame-rate independent, eased by construction.
    let alpha = 1.0 - (-target.speed * dt).exp();
    let next = current.lerp_towards(target.rotation, alpha);

    Ok(player.set_rotation(next.yaw, next.pitch)?)
}
