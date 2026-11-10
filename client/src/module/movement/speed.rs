use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Vanilla sprint speed, in blocks/tick (5.612 m/s ÷ 20) — the displacement
/// produced at multiplier `1.0`.
const SPRINT_SPEED: f64 = 0.2806;
/// The movement acceleration Minecraft adds, on its own, on top of the velocity
/// set here each tick. Subtracted so the resulting speed matches the target
/// instead of overshooting it.
const TICK_ACCEL: f64 = 0.13;
/// Horizontal speed below which the velocity direction is too ill-defined to
/// rescale.
const EPSILON: f64 = 0.001;

/// Moves the player faster than vanilla by rescaling their ground velocity.
///
/// Each tick — while a movement key is held — the horizontal velocity is set so
/// the next tick's displacement is `SPRINT_SPEED × multiplier`, keeping its
/// direction. Gating on real input (`xxa`/`zza`) is essential: without it the
/// boosted velocity would never decay and the player would glide on forever
/// after releasing the key.
#[derive(Debug)]
pub struct SpeedModule {
    pub module: ModuleData,
}

impl SpeedModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Speed,
                description: "Moves the player faster than vanilla".to_string(),
                category: ModuleCategory::Movement,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![ModuleSetting::Slider {
                    name: "Multiplier".to_string(),
                    value: 1.5,
                    min: 1.0,
                    max: 3.0,
                }],
            },
        }
    }

    fn multiplier(&self) -> f64 {
        self.module
            .get_setting("Multiplier")
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(1.5) as f64
    }
}

impl Module for SpeedModule {
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
        // Boost ground movement only, and only while a key is actually held —
        // the input gate is what lets the velocity decay on release.
        if !player.on_ground()? || !player.has_move_input()? {
            return Ok(());
        }

        let motion = player.get_delta_movement()?;
        let horizontal = (motion.x * motion.x + motion.z * motion.z).sqrt();
        if horizontal <= EPSILON {
            return Ok(()); // just started moving — direction not settled yet
        }

        // The velocity to set so that, once the game adds this tick's own
        // acceleration, the displacement lands on the target speed.
        let target = (SPRINT_SPEED * self.multiplier() - TICK_ACCEL).max(EPSILON);
        if horizontal >= target {
            return Ok(()); // already at or above the target — never slow down
        }

        let scale = target / horizontal;
        Ok(player.set_delta_movement(motion.x * scale, motion.y, motion.z * scale)?)
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
