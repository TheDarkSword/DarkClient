use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};
use crate::net::packet::{Packet, PacketAction};
use std::sync::atomic::{AtomicI32, Ordering};

/// Network-id sentinel meaning "the local player's id is not known yet".
const UNKNOWN_ID: i32 = i32::MIN;

/// Velocity — scales, or fully cancels, the knockback the server applies to the
/// player. The server pushes the player by sending a
/// `ClientboundSetEntityMotionPacket` for the player's own entity; this module
/// intercepts that packet and multiplies the motion by the configured
/// percentages. `Horizontal` / `Vertical` at 0 % is full anti-knockback; at
/// 100 % the motion passes through unchanged. The work happens in
/// [`VelocityModule::handle_packet`] — this module is packet-driven.
#[derive(Debug)]
pub struct VelocityModule {
    pub module: ModuleData,
    /// Network id of the local player, cached every tick so `handle_packet`
    /// (which runs on the Netty thread and does no JNI of its own) can match
    /// the motion packet against it.
    local_id: AtomicI32,
}

impl VelocityModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Velocity,
                description: "Reduces or cancels server knockback".to_string(),
                category: ModuleCategory::Combat,
                key_bind: KeyboardKey::KeyV,
                enabled: false,
                settings: vec![
                    ModuleSetting::Slider {
                        name: "Horizontal".to_string(),
                        value: 0.0,
                        min: 0.0,
                        max: 100.0,
                    },
                    ModuleSetting::Slider {
                        name: "Vertical".to_string(),
                        value: 0.0,
                        min: 0.0,
                        max: 100.0,
                    },
                ],
            },
            local_id: AtomicI32::new(UNKNOWN_ID),
        }
    }

    /// A percentage slider as a 0.0–1.0 multiplier.
    fn factor(&self, name: &str) -> f64 {
        self.module
            .get_setting(name)
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(0.0) as f64
            / 100.0
    }
}

impl Module for VelocityModule {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        self.local_id.store(UNKNOWN_ID, Ordering::Relaxed);
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        // Refresh the cached player id; `handle_packet` reads it cross-thread.
        let id = match minecraft().player()? {
            Some(player) => player.get_id()?,
            None => UNKNOWN_ID,
        };
        self.local_id.store(id, Ordering::Relaxed);
        Ok(())
    }

    fn handle_packet(&self, packet: &mut Packet) -> PacketAction {
        let Packet::ClientboundSetEntityMotion(motion) = packet else {
            return PacketAction::Forward;
        };
        // Only neuter knockback aimed at the local player.
        if motion.id != self.local_id.load(Ordering::Relaxed) {
            return PacketAction::Forward;
        }
        let horizontal = self.factor("Horizontal");
        let vertical = self.factor("Vertical");
        motion.movement.x *= horizontal;
        motion.movement.y *= vertical;
        motion.movement.z *= horizontal;
        PacketAction::Forward
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
