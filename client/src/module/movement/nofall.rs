use crate::error::Result;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};
use crate::net::packet::{Packet, PacketAction};

/// Prevents fall damage. The work happens in [`NoFallModule::handle_packet`]:
/// while enabled, every outbound movement packet reports the player as on the
/// ground, so the server never accumulates the fall distance it would turn
/// into damage. `on_tick` does nothing — this module is packet-driven.
#[derive(Debug)]
pub struct NoFallModule {
    pub module: ModuleData,
}

impl NoFallModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::NoFall,
                description: "Prevents fall damage".to_string(),
                category: ModuleCategory::Movement,
                key_bind: KeyboardKey::KeyN,
                enabled: false,
                settings: vec![],
            },
        }
    }
}

impl Module for NoFallModule {
    fn on_start(&self) -> Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> Result<()> {
        Ok(())
    }

    fn on_tick(&self) -> Result<()> {
        Ok(())
    }

    fn handle_packet(&self, packet: &mut Packet) -> PacketAction {
        // Report the player as on the ground on every movement packet, so the
        // server never accumulates the fall distance it turns into damage.
        if let Packet::ServerboundMovePlayer(move_packet) = packet {
            move_packet.on_ground = true;
        }
        PacketAction::Forward
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
