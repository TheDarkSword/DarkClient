use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};
use crate::net::packet::{Packet, PacketAction};
use std::sync::Mutex;

/// Active Freecam state — present only while the module is running.
#[derive(Debug)]
struct FreecamState {
    /// The body's real position — restored on disable. Outbound movement
    /// packets are dropped while active, so the server keeps the player here.
    anchor: (f64, f64, f64),
    /// The body's yaw/pitch at the moment Freecam was switched on. Restored
    /// on disable so the player is not left looking wherever the camera
    /// stopped.
    anchor_yaw: f32,
    anchor_pitch: f32,
    /// `PlayerInfo.gameMode` at the moment Freecam was switched on — used to
    /// restore the real game mode on disable.
    previous_game_mode: GameType,
    /// Whether the player was flying before — restored on disable.
    previous_flying: bool,
    /// Inbound `ClientboundPlayerInfoUpdatePacket`s caught while active. They
    /// would otherwise overwrite our spectator override; instead they are
    /// queued here and replayed in arrival order when Freecam stops.
    queued_info_updates: Vec<Object>,
}

/// Detaches the view by faking spectator mode client-side and silencing
/// movement packets:
///
/// * `PlayerInfo.gameMode` is set to `SPECTATOR`, so `Player.isSpectator()` is
///   true. `Player.tick` then keeps `noPhysics` set every tick (no view-blocking
///   overlay) and the body has no block collision.
/// * The fly abilities are set, so the player flies instead of falling.
/// * Every outbound `ServerboundMovePlayerPacket` is dropped, so the server
///   keeps seeing the body at the anchor.
/// * Inbound `ClientboundPlayerInfoUpdatePacket`s — which would tear down the
///   spectator override — are queued and replayed only when Freecam stops, so
///   the client's view of the player list catches up cleanly afterwards.
#[derive(Debug)]
pub struct FreecamModule {
    pub module: ModuleData,
    state: Mutex<Option<FreecamState>>,
}

impl FreecamModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Freecam,
                description: "Flies the view freely while the body stays put".to_string(),
                category: ModuleCategory::Player,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
            state: Mutex::new(None),
        }
    }
}

fn player_info(player: &LocalPlayer) -> anyhow::Result<PlayerInfo> {
    player
        .player_info()?
        .ok_or_else(|| anyhow::anyhow!("LocalPlayer.playerInfo is null"))
}

fn set_flying(player: &LocalPlayer, flying: bool) -> anyhow::Result<()> {
    let abilities = player.get_abilities()?;
    abilities.set_flying(flying)?;
    abilities.set_mayfly(flying)?;
    Ok(())
}

/// Delivers a held packet the way `Connection.channelRead` would.
fn replay(player: &LocalPlayer, packet: &Object) -> anyhow::Result<()> {
    let Some(listener) = player.connection()? else {
        return Ok(());
    };
    let packet: mc_bindings::Packet = packet
        .cast()?
        .ok_or_else(|| anyhow::anyhow!("not a packet"))?;
    Ok(packet.handle(&listener)?)
}

impl Module for FreecamModule {
    fn on_start(&self) -> anyhow::Result<()> {
        let Some(player) = minecraft().player()? else {
            return Ok(());
        };
        let feet = player.position()?;
        let anchor_yaw = player.get_y_rot()?;
        let anchor_pitch = player.get_x_rot()?;
        let player_info = player_info(&player)?;
        let previous_game_mode = player_info.game_mode()?;
        let previous_flying = player.get_abilities()?.flying()?;

        // Switch the client to spectator without telling the server.
        player_info.set_game_mode(GameType::Spectator)?;
        set_flying(&player, true)?;

        *self.state.lock().unwrap() = Some(FreecamState {
            anchor: (feet.x, feet.y, feet.z),
            anchor_yaw,
            anchor_pitch,
            previous_game_mode,
            previous_flying,
            queued_info_updates: Vec::new(),
        });
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        let Some(state) = self.state.lock().unwrap().take() else {
            return Ok(());
        };
        if let Some(player) = minecraft().player()? {
            // Restore the real game mode + flight, then snap the body back to
            // where the server has been keeping it.
            if let Ok(info) = player_info(&player) {
                let _ = info.set_game_mode(state.previous_game_mode);
            }
            set_flying(&player, state.previous_flying)?;
            player.set_pos(state.anchor.0, state.anchor.1, state.anchor.2)?;
            player.set_rotation(state.anchor_yaw, state.anchor_pitch)?;
            player.set_delta_movement(0.0, 0.0, 0.0)?;

            // Replay every queued PlayerInfo update so the client catches up
            // to anything the server announced (game-mode changes, latency, …).
            for packet in &state.queued_info_updates {
                if let Err(error) = replay(&player, packet) {
                    log::warn!("Freecam: replay of a queued packet failed: {error}");
                }
            }
        }
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn handle_packet(&self, packet: &mut Packet) -> PacketAction {
        match packet {
            // Drop every outbound movement update — the server must keep
            // seeing the body where Freecam was switched on.
            Packet::ServerboundMovePlayer(_) => PacketAction::Cancel,
            // Hold every inbound player-info update so it cannot stomp on the
            // spectator override; replay them in arrival order on disable.
            Packet::ClientboundPlayerInfoUpdate(captured) => {
                if let Ok(mut guard) = self.state.lock() {
                    if let Some(state) = guard.as_mut() {
                        state.queued_info_updates.push(captured.packet.clone());
                        return PacketAction::Cancel;
                    }
                }
                PacketAction::Forward
            }
            _ => PacketAction::Forward,
        }
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
