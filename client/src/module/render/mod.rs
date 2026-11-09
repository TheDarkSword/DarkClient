//! Render-category modules.
//!
//! These modules are pure configuration holders: the actual drawing is done
//! every frame by [`crate::graphic::esp`] (and, for the module list, by
//! [`crate::graphic::hud`]), which reads their enabled state and settings. Keeping the JNI/render work in one place is what lets the ESP run
//! without costing frame rate.

pub mod arraylist;
pub mod chest_esp;
pub mod fullbright;
pub mod mob_esp;
pub mod nametags;
pub mod player_esp;
pub mod tracers;
