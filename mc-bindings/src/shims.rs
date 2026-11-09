//! Hand-written members, for versions where a spec member has no direct
//! counterpart. Each is named by a `[[shim]]` in `mc-data/adapters/` and takes
//! the same arguments as the generated function it stands in for.

use jni::objects::JObject;
use mc_runtime::{Cx, Result};

use crate::{Gui, ItemStack, LivingEntity, Minecraft};

/// `Gui.screen()` before 26.2, where the open screen was `Minecraft.screen`.
pub(crate) fn gui_screen_from_minecraft<'l>(
    cx: &mut Cx<'l>,
    gui: &JObject<'_>,
) -> Result<JObject<'l>> {
    let minecraft = Gui::minecraft(cx, gui)?;
    Minecraft::screen(cx, &minecraft)
}

/// `LivingEntity.swing(hand)` from 26.3, where a swing carries the held
/// item's attack animation — as `Minecraft.startAttack` passes it.
pub(crate) fn swing_with_attack_animation(
    cx: &mut Cx<'_>,
    entity: &JObject<'_>,
    hand: &JObject<'_>,
) -> Result<()> {
    let stack = LivingEntity::get_item_in_hand(cx, entity, hand)?;
    let animation = ItemStack::get_attack_animation(cx, &stack)?;
    LivingEntity::swing_with_animation(cx, entity, hand, &animation, false)?;
    Ok(())
}
