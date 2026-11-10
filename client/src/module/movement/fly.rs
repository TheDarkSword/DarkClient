use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Vanilla creative-fly speed — restored when Fly is turned off.
const VANILLA_FLY_SPEED: f32 = 0.05;

#[derive(Debug)]
pub struct FlyModule {
    pub module: ModuleData,
}

impl FlyModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Fly,
                description: "Enables flight".to_string(),
                category: ModuleCategory::Movement,
                key_bind: KeyboardKey::KeyF,
                enabled: false,
                settings: vec![ModuleSetting::Slider {
                    name: "Speed".to_string(),
                    value: 1.0,
                    min: 0.1,
                    max: 5.0,
                }],
            },
        }
    }

    /// The configured speed as an `Abilities.flyingSpeed` value (the slider is
    /// a multiplier over the vanilla speed).
    fn fly_speed(&self) -> f32 {
        let multiplier = self
            .module
            .get_setting("Speed")
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(1.0);
        VANILLA_FLY_SPEED * multiplier
    }
}

fn set_flight(enabled: bool, speed: f32) -> anyhow::Result<()> {
    if let Some(player) = minecraft().player()? {
        let abilities = player.get_abilities()?;
        abilities.set_flying(enabled)?;
        abilities.set_mayfly(enabled)?;
        abilities.set_flying_speed(speed)?;
    }
    Ok(())
}

impl Module for FlyModule {
    fn on_start(&self) -> anyhow::Result<()> {
        set_flight(true, self.fly_speed())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        set_flight(false, VANILLA_FLY_SPEED)
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        // Re-assert each tick so a server-sent abilities update cannot quietly
        // disable flight or reset the speed.
        set_flight(true, self.fly_speed())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
