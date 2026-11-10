use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};

/// Trailing menu slots that are the player's own inventory (27 storage + 9
/// hotbar) — present on every standard container menu.
const PLAYER_INVENTORY_SLOTS: usize = 36;

/// Empties an open container into the player's inventory, one stack per tick
/// (shift-clicking the whole chest at once would flood the server with clicks).
#[derive(Debug)]
pub struct ChestStealerModule {
    pub module: ModuleData,
}

impl ChestStealerModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::ChestStealer,
                description: "Empties an open container into the inventory".to_string(),
                category: ModuleCategory::Player,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
        }
    }
}

impl Module for ChestStealerModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        let Some(player) = minecraft().player()? else {
            return Ok(());
        };

        // Container id 0 is the player's own inventory menu — nothing to steal.
        let menu = player.container_menu()?;
        let container_id = menu.container_id()?;
        if container_id == 0 {
            return Ok(());
        }

        // The container's own slots come before the player-inventory tail.
        let items = menu.get_items()?;
        let container_slots = items.len().saturating_sub(PLAYER_INVENTORY_SLOTS);

        // Move the first non-empty container slot — one per tick.
        for (slot, item) in items.iter().enumerate().take(container_slots) {
            if !item.is_empty()? {
                let Some(game_mode) = minecraft().game_mode()? else {
                    return Ok(());
                };
                game_mode.handle_container_input(
                    container_id,
                    slot as i32,
                    0,
                    ContainerInput::QuickMove,
                    &player,
                )?;
                return Ok(());
            }
        }
        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
