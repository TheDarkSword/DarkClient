use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};

/// `UseEffects.DEFAULT.speedMultiplier` (`UseEffects.java`) — the factor the
/// item-use slowdown scales movement input by.
const USE_ITEM_MULTIPLIER: f64 = 0.2;
/// Acceleration to add back, as a multiple of the slowed acceleration the game
/// applied this tick, so the total reaches the un-slowed amount
/// (`1/0.2 − 1 = 4`).
const COMPENSATION: f64 = 1.0 / USE_ITEM_MULTIPLIER - 1.0;
/// `getFrictionInfluencedSpeed` factor on a normal-friction (0.6) block:
/// `0.21600002 / 0.6³` (`LivingEntity.getFrictionInfluencedSpeed`).
const GROUND_SPEED_FACTOR: f64 = 0.21600002 / (0.6 * 0.6 * 0.6);
/// Mirrors `getInputVector`'s `lengthSqr < 1.0E-7` zero-input guard.
const INPUT_EPSILON: f64 = 1.0e-7;

/// Cancels the movement slowdown Minecraft applies while an item is in use
/// (eating, drawing a bow, blocking).
///
/// The slowdown scales the movement input to `0.2×` (`UseEffects.speedMultiplier`)
/// before it accelerates the player, and there is no hook to skip it. So it is
/// undone after the fact: every tick the *exact* acceleration that input would
/// have produced un-slowed is reconstructed from `getInputVector`'s formula and
/// the missing `4×` is added back to the velocity. Only the acceleration is
/// touched — momentum and friction are left to the game, so releasing the keys
/// still stops the player naturally.
#[derive(Debug)]
pub struct NoSlowdownModule {
    pub module: ModuleData,
}

impl NoSlowdownModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::NoSlowdown,
                description: "Removes the slowdown from using items".to_string(),
                category: ModuleCategory::Movement,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
        }
    }
}

impl Module for NoSlowdownModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        let Some(player) = minecraft().player()? else {
            return Ok(());
        };
        // The slowdown only applies on the ground; air movement uses a
        // different acceleration that this compensation would not match.
        if !player.is_using_item()? || !player.on_ground()? {
            return Ok(());
        }

        let (strafe, forward) = player.move_input()?;
        let input_sq = (strafe * strafe + forward * forward) as f64;
        if input_sq < INPUT_EPSILON {
            return Ok(()); // no input — the game added no acceleration to undo
        }

        // Reconstruct the (slowed) acceleration the game added this tick, with
        // `getInputVector`'s formula: scale the local input by the ground
        // speed, then rotate it by the player's yaw into world space.
        let speed = player.get_speed()? as f64 * GROUND_SPEED_FACTOR;
        let (sin, cos) = (player.get_y_rot()? as f64).to_radians().sin_cos();
        let local_x = strafe as f64 * speed;
        let local_z = forward as f64 * speed;
        let accel_x = local_x * cos - local_z * sin;
        let accel_z = local_z * cos + local_x * sin;

        // Add the missing acceleration so the player moves as if un-slowed.
        let motion = player.get_delta_movement()?;
        Ok(player.set_delta_movement(
            motion.x + accel_x * COMPENSATION,
            motion.y,
            motion.z + accel_z * COMPENSATION,
        )?)
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
