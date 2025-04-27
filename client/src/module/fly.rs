use crate::mapping::entity::player::LocalPlayer;
use crate::module::{FlyModule, KeyboardKey, Module, ModuleCategory, ModuleData};

impl FlyModule {
    pub fn new(player: LocalPlayer) -> Self {
        Self {
            module: ModuleData {
                name: "Fly".to_string(),
                description: "Enables flying".to_string(),
                category: ModuleCategory::MOVEMENT,
                key_bind: KeyboardKey::KeyF,
                enabled: false,
                player,
            }
        }
    }
}

impl Module for FlyModule {
    
    fn on_start(&self) {
        // Enables flying
        self.module.player.abilities.fly(true);
    }

    fn on_stop(&self) {
        // Disables flying
        self.module.player.abilities.fly(false);
    }

    fn on_tick(&self) {
        // No operation
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }
    
    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}