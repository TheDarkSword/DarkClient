use crate::game::prelude::*;
use crate::module::combat::{look_at, pick_target};
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};
use std::sync::Mutex;

#[derive(Debug)]
pub struct AimbotModule {
    pub module: ModuleData,
    /// Network id of the locked target, if any.
    target: Mutex<Option<i32>>,
}

impl AimbotModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Aimbot,
                description: "Smoothly aims at the nearest entity".to_string(),
                category: ModuleCategory::Combat,
                key_bind: KeyboardKey::KeyC,
                enabled: false,
                settings: vec![
                    ModuleSetting::Slider {
                        name: "Range".to_string(),
                        value: 4.0,
                        min: 2.0,
                        max: 8.0,
                    },
                    ModuleSetting::Slider {
                        name: "FOV".to_string(),
                        value: 100.0,
                        min: 10.0,
                        max: 180.0,
                    },
                    ModuleSetting::Slider {
                        name: "Speed".to_string(),
                        value: 7.0,
                        min: 2.0,
                        max: 20.0,
                    },
                ],
            },
            target: Mutex::new(None),
        }
    }

    fn slider(&self, name: &str, fallback: f32) -> f32 {
        self.module
            .get_setting(name)
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(fallback)
    }

    fn range(&self) -> f32 {
        self.slider("Range", 4.0)
    }

    fn fov(&self) -> f32 {
        self.slider("FOV", 100.0)
    }

    fn speed(&self) -> f32 {
        self.slider("Speed", 7.0)
    }
}

impl Module for AimbotModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        *self.target.lock().unwrap() = None;
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        let minecraft = minecraft();
        // Stand down while a menu (inventory, chest, crafting, chat, …) is open.
        if minecraft.current_screen().is_open() {
            return Ok(());
        }
        let (Some(player), Some(world)) = (minecraft.player()?, minecraft.level()?) else {
            *self.target.lock().unwrap() = None;
            return Ok(()); // not in a world — nothing to do
        };

        let entities = world.entities_for_rendering()?;
        let eye = player.get_eye_position()?;
        let self_id = player.get_id()?;
        let range = self.range() as f64;
        let locked = *self.target.lock().unwrap();

        let Some((target_id, target)) =
            pick_target(&entities, eye, range * range, self_id, locked, |entity| {
                entity.instance_of::<Player>() || entity.instance_of::<Mob>()
            })
        else {
            *self.target.lock().unwrap() = None;
            return Ok(());
        };

        let angle = look_at(&player, &target, self.speed(), self.fov())?;
        // A returned angle past the FOV means the target was out of view and
        // nothing was rotated — release the lock so a better one can be picked.
        *self.target.lock().unwrap() = if angle > self.fov() {
            None
        } else {
            Some(target_id)
        };

        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
