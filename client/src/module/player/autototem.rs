use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};

/// Container id of the player's own inventory menu — always `0`.
const INVENTORY_MENU_ID: i32 = 0;
/// `Inventory` index of the off-hand. Doubles as the SWAP button code that
/// targets the off-hand.
const OFFHAND_SLOT: i32 = SLOT_OFFHAND;
/// `Inventory` indices scanned for a spare totem — hotbar + main storage.
const STORAGE_SLOTS: i32 = 36;

/// Keeps a Totem of Undying in the off-hand: whenever the off-hand totem is
/// missing (just popped, or never equipped) a spare one from the inventory is
/// swapped in.
#[derive(Debug)]
pub struct AutoTotemModule {
    pub module: ModuleData,
}

impl AutoTotemModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::AutoTotem,
                description: "Keeps a Totem of Undying in the off-hand".to_string(),
                category: ModuleCategory::Player,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
        }
    }
}

impl Module for AutoTotemModule {
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

        // Act only on the player's own inventory menu: an external container
        // (chest, …) has different slot numbering, so stand down while one is
        // open.
        if player.container_menu()?.container_id()? != INVENTORY_MENU_ID {
            return Ok(());
        }

        let inventory = player.get_inventory()?;
        let totem = Items::totem_of_undying()?;

        // Already holding a totem — nothing to do.
        if slot_holds(&inventory, OFFHAND_SLOT, &totem)? {
            return Ok(());
        }

        // Find a spare totem in the hotbar / main storage.
        let Some(inventory_slot) = find_totem(&inventory, &totem)? else {
            return Ok(()); // no totem anywhere — give up until next tick
        };

        // Swap it into the off-hand through the inventory menu.
        let Some(game_mode) = minecraft().game_mode()? else {
            return Ok(());
        };
        game_mode.handle_container_input(
            INVENTORY_MENU_ID,
            menu_slot(inventory_slot),
            OFFHAND_SLOT,
            ContainerInput::Swap,
            &player,
        )?;
        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}

/// Whether `slot` of `inventory` holds the item `item`.
fn slot_holds(inventory: &Inventory, slot: i32, item: &Item) -> anyhow::Result<bool> {
    let stack = inventory.get_item(slot)?;
    if stack.is_empty()? {
        return Ok(false);
    }
    Ok(stack.get_item()?.is_same(item))
}

/// The first hotbar / main-storage slot holding a totem, if any.
fn find_totem(inventory: &Inventory, totem: &Item) -> anyhow::Result<Option<i32>> {
    for slot in 0..STORAGE_SLOTS {
        if slot_holds(inventory, slot, totem)? {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}
