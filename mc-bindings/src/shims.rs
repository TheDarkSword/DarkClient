//! Members written by hand where a version has no direct equivalent; each one
//! is named by a `[[shim]]` in `mc-data/adapters/`.

use crate::{Gui, InteractionHand, LivingEntity, Result, Screen};

// Before 26.2 the open screen was `Minecraft.screen`.
pub(crate) fn gui_screen_from_minecraft(gui: &Gui) -> Result<Option<Screen>> {
    gui.minecraft()?.screen()
}

// From 26.3 a swing takes the held item's attack animation.
pub(crate) fn swing_with_attack_animation(
    entity: &LivingEntity,
    hand: InteractionHand,
) -> Result<()> {
    let animation = entity.get_item_in_hand(hand)?.get_attack_animation()?;
    entity.swing_with_animation(hand, &animation, false)?;
    Ok(())
}
