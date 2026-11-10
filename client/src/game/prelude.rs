//! One import for modules: `use crate::game::prelude::*;`.

#![allow(unused_imports)]

pub use super::screen::{MinecraftExt, ScreenKind};
pub use super::{
    menu_slot, BlockEntityExt, BlockPosExt, ClientLevelExt, DirectionExt, EntityExt, GameModeExt,
    LivingEntityExt, ObjectExt, OptionInstanceExt, SLOT_OFFHAND,
};
pub use crate::state::minecraft;
pub use mc_bindings::*;
// The generated `Result<T>` must not hide the standard one.
pub use std::result::Result;
