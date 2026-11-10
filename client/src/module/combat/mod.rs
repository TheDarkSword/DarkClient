pub mod aimbot;
pub mod aura;
pub mod criticals;
pub mod killaura;
pub mod mobaura;
pub mod rotation;
pub mod velocity;

use crate::game::prelude::*;
use rotation::Rotation;

/// Aims the camera at `target` — eased, through the shared rotation controller
/// — when the target is within `max_fov` degrees of the current look. Returns
/// the angle, in degrees, currently between the look direction and the target,
/// which combat modules use to decide when they are aligned enough to attack.
pub fn look_at(
    player: &LocalPlayer,
    target: &Entity,
    speed: f32,
    max_fov: f32,
) -> anyhow::Result<f32> {
    let eye = player.get_eye_position()?;
    let feet = target.position()?;
    let height = target.get_bb_height()? as f64;
    // Aim at the upper body — reliable hit registration, natural-looking.
    let aim = Vec3::new(feet.x, feet.y + height * 0.7, feet.z);
    let target_rotation = Rotation::towards(eye, aim);

    let current = Rotation::new(player.get_y_rot()?, player.get_x_rot()?);
    let angle = current.angle_to(target_rotation);
    if angle <= max_fov {
        rotation::aim(target_rotation, speed);
    }
    Ok(angle)
}

/// Picks a combat target from `entities`: keeps the `locked` target while it is
/// still a valid in-range candidate, otherwise the nearest one. `accept`
/// filters by kind; `exclude_id` drops the player's own entity. Returns the
/// chosen entity's network id together with the entity.
pub fn pick_target(
    entities: &[Entity],
    eye: Vec3,
    range_sq: f64,
    exclude_id: i32,
    locked: Option<i32>,
    accept: impl Fn(&Entity) -> bool,
) -> Option<(i32, Entity)> {
    let mut candidates: Vec<(i32, Entity, f64)> = Vec::new();
    for entity in entities {
        let distance = match entity.distance_to_sqr(eye.x, eye.y, eye.z) {
            Ok(distance) if distance <= range_sq => distance,
            _ => continue,
        };
        if !accept(entity) {
            continue;
        }
        let Ok(id) = entity.get_id() else {
            continue;
        };
        if id == exclude_id {
            continue;
        }
        candidates.push((id, entity.clone(), distance));
    }

    // Keep the locked target while it is still in the candidate set.
    if let Some(locked) = locked {
        if let Some((id, entity, _)) = candidates.iter().find(|(id, _, _)| *id == locked) {
            return Some((*id, entity.clone()));
        }
    }
    candidates
        .into_iter()
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(id, entity, _)| (id, entity))
}
