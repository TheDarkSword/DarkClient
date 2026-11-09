use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Highlights mobs (hostile and passive creatures) with a 3D wireframe box.
///
/// All rendering lives in [`crate::graphic::esp`]; this struct only carries the
/// toggle state and the visual settings.
#[derive(Debug)]
pub struct MobEspModule {
    pub module: ModuleData,
}

impl MobEspModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::MobEsp,
                description: "Draws a 3D box around mobs".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![
                    ModuleSetting::Color {
                        name: "Color".to_string(),
                        value: [1.0, 0.84, 0.2, 1.0],
                        alpha: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Name".to_string(),
                        value: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Distance".to_string(),
                        value: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Health".to_string(),
                        value: true,
                    },
                    ModuleSetting::Slider {
                        name: "Range".to_string(),
                        value: 64.0,
                        min: 16.0,
                        max: 256.0,
                    },
                ],
            },
        }
    }
}

impl Module for MobEspModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
