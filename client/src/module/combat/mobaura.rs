use crate::error::Result;
use crate::module::combat::aura::{AuraTarget, BaseAura};
use crate::module::{KeyboardKey, Module, ModuleData, ModuleId};

#[derive(Debug)]
pub struct MobAuraModule {
    pub aura: BaseAura,
}

impl MobAuraModule {
    pub fn new() -> Self {
        Self {
            aura: BaseAura::new(
                ModuleId::MobAura,
                "Automatically attacks mobs".to_string(),
                KeyboardKey::KeyY,
                AuraTarget::Mobs,
            ),
        }
    }
}

impl Module for MobAuraModule {
    fn on_start(&self) -> Result<()> {
        self.aura.on_start()
    }

    fn on_stop(&self) -> Result<()> {
        self.aura.on_stop()
    }

    fn on_tick(&self) -> Result<()> {
        self.aura.on_tick()
    }

    fn get_module_data(&self) -> &ModuleData {
        self.aura.get_module_data()
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        self.aura.get_module_data_mut()
    }
}
