//! Criticals — makes the combat auras land critical hits.
//!
//! A hit only registers as critical when the attacker is airborne and
//! *descending* (`fallDistance > 0`). When armed, this module hops the player
//! off the ground and the aura holds its hit until that window opens — see
//! [`prepare`], which the auras call right before they attack.

use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};
use crate::state::client;

/// Upward velocity of the crit hop — Minecraft's vanilla jump velocity.
const HOP_VELOCITY: f64 = 0.42;

/// Whether to attack now, or hold the hit while a crit is set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CritState {
    /// Attack now.
    Ready,
    /// Hold the hit — the player is being hopped into the crit window.
    Charging,
}

/// Whether the Criticals module is currently armed.
fn enabled() -> bool {
    client()
        .modules
        .get(ModuleId::Criticals)
        .and_then(|arc| {
            arc.lock()
                .ok()
                .map(|module| module.get_module_data().enabled)
        })
        .unwrap_or(false)
}

/// Called by a combat aura just before it attacks.
///
/// With Criticals off this is always [`CritState::Ready`]. With it on, the
/// player is hopped off the ground and the hit held ([`CritState::Charging`])
/// until they are airborne and descending — the window in which a hit
/// registers as a critical.
pub fn prepare(player: &LocalPlayer) -> anyhow::Result<CritState> {
    if !enabled() {
        return Ok(CritState::Ready);
    }
    if player.on_ground()? {
        // Hop straight up, keeping the current horizontal motion.
        let motion = player.get_delta_movement()?;
        player.set_delta_movement(motion.x, HOP_VELOCITY, motion.z)?;
        return Ok(CritState::Charging);
    }
    // Airborne: a hit only crits while descending — i.e. once fall distance
    // has begun to accumulate.
    if player.fall_distance()? > 0.0 {
        Ok(CritState::Ready)
    } else {
        Ok(CritState::Charging)
    }
}

/// Highlights nothing and draws nothing — a pure modifier consulted by the
/// combat auras through [`prepare`].
#[derive(Debug)]
pub struct CriticalsModule {
    pub module: ModuleData,
}

impl CriticalsModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Criticals,
                description: "Makes the combat auras land critical hits".to_string(),
                category: ModuleCategory::Combat,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
        }
    }
}

impl Module for CriticalsModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
