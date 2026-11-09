use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};

/// Highlights containers — chests, trapped chests, ender chests, barrels and
/// shulker boxes — with a 3D wireframe box.
///
/// All rendering lives in [`crate::graphic::esp`]; this struct only carries the
/// toggle state and the visual settings.
#[derive(Debug)]
pub struct ChestEspModule {
    pub module: ModuleData,
}

impl ChestEspModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::ChestEsp,
                description: "Draws a 3D box around containers".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![
                    ModuleSetting::Color {
                        name: "Color".to_string(),
                        value: [1.0, 0.55, 0.12, 1.0],
                        alpha: true,
                    },
                    ModuleSetting::Toggle {
                        name: "Distance".to_string(),
                        value: true,
                    },
                ],
            },
        }
    }
}

impl Module for ChestEspModule {
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
