use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Keeps the connection alive by nudging the player's view at a fixed interval,
/// so an idle-kick timer never expires.
///
/// Each action flips the yaw a small amount, alternating direction so the view
/// jitters back and forth without drifting. A rotation change makes Minecraft
/// emit a movement packet, which is what the server sees as activity.
#[derive(Debug)]
pub struct AntiAfkModule {
    pub module: ModuleData,
    /// When the last nudge fired.
    last_action: Mutex<Option<Instant>>,
    /// Direction of the next yaw nudge — flipped each action.
    positive: Mutex<bool>,
}

impl AntiAfkModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::AntiAfk,
                description: "Nudges the view periodically to avoid idle kicks".to_string(),
                category: ModuleCategory::Misc,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![
                    ModuleSetting::Slider {
                        name: "Interval".to_string(),
                        value: 6.0,
                        min: 1.0,
                        max: 30.0,
                    },
                    ModuleSetting::Slider {
                        name: "Strength".to_string(),
                        value: 12.0,
                        min: 1.0,
                        max: 45.0,
                    },
                ],
            },
            last_action: Mutex::new(None),
            positive: Mutex::new(true),
        }
    }

    fn slider(&self, name: &str, fallback: f32) -> f32 {
        self.module
            .get_setting(name)
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(fallback)
    }
}

impl Module for AntiAfkModule {
    fn on_start(&self) -> anyhow::Result<()> {
        // Start the interval now, so the first nudge waits a full interval.
        *self.last_action.lock().unwrap() = Some(Instant::now());
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        *self.last_action.lock().unwrap() = None;
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        let interval = Duration::from_secs_f32(self.slider("Interval", 6.0).max(0.5));
        let now = Instant::now();
        {
            let mut last = self.last_action.lock().unwrap();
            if last.is_some_and(|t| now.duration_since(t) < interval) {
                return Ok(());
            }
            *last = Some(now);
        }

        let Some(player) = minecraft().player()? else {
            return Ok(());
        };

        let strength = self.slider("Strength", 12.0);
        let delta = {
            let mut positive = self.positive.lock().unwrap();
            let signed = if *positive { strength } else { -strength };
            *positive = !*positive;
            signed
        };

        let yaw = player.get_y_rot()?;
        let pitch = player.get_x_rot()?;
        Ok(player.set_rotation(yaw + delta, pitch)?)
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
