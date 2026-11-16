use crate::error::Result;
use crate::module::combat::aura::{AuraTarget, BaseAura};
use crate::module::{KeyboardKey, Module, ModuleData, ModuleId};

#[derive(Debug)]
pub struct KillAuraModule {
    pub aura: BaseAura,
}

impl KillAuraModule {
    pub fn new() -> Self {
        Self {
            aura: BaseAura::new(
                ModuleId::KillAura,
                "Automatically attacks players".to_string(),
                KeyboardKey::KeyR,
                AuraTarget::Players,
            ),
        }
    }
}

impl Module for KillAuraModule {
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
