use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};

/// Shows the list of enabled modules in the top-right corner of the HUD.
///
/// A pure on/off switch: the list itself is drawn by [`crate::graphic::hud`],
/// which checks this module's enabled flag — so it also applies at the main
/// menu, where modules are otherwise inactive.
#[derive(Debug)]
pub struct ArrayListModule {
    pub module: ModuleData,
}

impl ArrayListModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::ArrayList,
                description: "Lists the enabled modules on the HUD".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: true,
                settings: Vec::new(),
            },
        }
    }
}

impl Module for ArrayListModule {
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
