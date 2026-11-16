//! One import for modules: `use crate::game::prelude::*;`.

#![allow(unused_imports)]

pub use super::screen::{MinecraftExt, ScreenKind};
pub use super::{
    menu_slot, BlockEntityExt, BlockPosExt, ClientLevelExt, DirectionExt, EntityExt, GameModeExt,
    LivingEntityExt, ObjectExt, OptionInstanceExt, SLOT_OFFHAND,
};
pub use crate::error::{Error, Result};
pub use crate::state::minecraft;
// The explicit `Error` / `Result` above win over the generated ones.
pub use mc_bindings::*;
