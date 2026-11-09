use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Draws a line from the bottom of the screen to every nearby entity.
///
/// Like the ESP modules, this struct is a pure configuration holder: the
/// drawing is done every frame by [`crate::graphic::esp`], which reuses the
/// same entity gather and projection used for the wireframe boxes.
#[derive(Debug)]
pub struct TracersModule {
    pub module: ModuleData,
}

impl TracersModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Tracers,
                description: "Draws lines pointing at nearby entities".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![
                    ModuleSetting::Color {
                        name: "Color".to_string(),
                        value: [0.4, 0.8, 1.0, 1.0],
                        alpha: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Players".to_string(),
                        value: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Mobs".to_string(),
                        value: false,
                    },
                    ModuleSetting::Slider {
                        name: "Range".to_string(),
                        value: 96.0,
                        min: 16.0,
                        max: 256.0,
                    },
                ],
            },
        }
    }
}

impl Module for TracersModule {
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
