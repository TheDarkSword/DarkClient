use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Draws a floating name tag above every nearby player.
///
/// Like the ESP modules, this struct is a pure configuration holder: the
/// drawing is done every frame by [`crate::graphic::esp`], reusing the same
/// entity gather and projection.
#[derive(Debug)]
pub struct NametagsModule {
    pub module: ModuleData,
}

impl NametagsModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Nametags,
                description: "Shows a name tag above nearby players".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![
                    ModuleSetting::Color {
                        name: "Color".to_string(),
                        value: [1.0, 1.0, 1.0, 1.0],
                        alpha: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Health".to_string(),
                        value: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Distance".to_string(),
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

impl Module for NametagsModule {
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
