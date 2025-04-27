use crate::mapping::entity::player::LocalPlayer;
use crate::module::{FlyModule, KeyboardKey, Module, ModuleData};

impl FlyModule {
    pub fn new(player: LocalPlayer) -> Self {
        Self {
            module: ModuleData {
                name: "Fly".to_string(),
                description: "Enables flying".to_string(),
                key_bind: KeyboardKey::KeyF,
                enabled: false,
                player,
            }
        }
    }
}

impl Module for FlyModule {
    
    fn on_start(&self) {
        // Abilita il volo
        //self.player.fly(true);
    }

    fn on_stop(&self) {
        // Disabilita il volo
        //self.player.fly(false);
    }

    fn on_tick(&self) {
        // Nessuna logica necessaria per tick
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }
    
    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}