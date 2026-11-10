//! Packets modules can read, change or cancel; a changed one is rebuilt into a
//! new JVM object.

pub mod clientbound_player_info_update;
pub mod serverbound_move_player;

use clientbound_player_info_update::ClientboundPlayerInfoUpdatePacket;
use jni::objects::JObject;
pub use mc_bindings::ClientboundSetEntityMotionPacket;
use mc_bindings::{JavaType, Object};
use mc_runtime::Cx;
use serverbound_move_player::ServerboundMovePlayerPacket;

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    ServerboundMovePlayer(ServerboundMovePlayerPacket),
    ClientboundSetEntityMotion(ClientboundSetEntityMotionPacket),
    /// Held as is, so Freecam can queue and replay it.
    ClientboundPlayerInfoUpdate(ClientboundPlayerInfoUpdatePacket),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacketAction {
    #[default]
    Forward,
    /// One module cancelling is enough; the rest are skipped.
    Cancel,
}

// Checked on the local ref, so packets nobody handles never get a global one.
fn is<T: JavaType>(packet: &JObject) -> anyhow::Result<bool> {
    Ok(Cx::current()?.is_instance_of(packet, T::class_name()?)?)
}

fn handle(packet: &JObject) -> anyhow::Result<Object> {
    Object::from_local(packet)?.ok_or_else(|| anyhow::anyhow!("null packet"))
}

impl Packet {
    pub fn from_outbound(packet: &JObject) -> anyhow::Result<Option<Packet>> {
        if is::<mc_bindings::ServerboundMovePlayerPacket>(packet)? {
            let packet: mc_bindings::ServerboundMovePlayerPacket = handle(packet)?
                .cast()?
                .ok_or_else(|| anyhow::anyhow!("not a ServerboundMovePlayerPacket"))?;
            return Ok(Some(Packet::ServerboundMovePlayer(
                ServerboundMovePlayerPacket::read(&packet)?,
            )));
        }
        Ok(None)
    }

    pub fn from_inbound(packet: &JObject) -> anyhow::Result<Option<Packet>> {
        if is::<ClientboundSetEntityMotionPacket>(packet)? {
            return Ok(Some(Packet::ClientboundSetEntityMotion(
                ClientboundSetEntityMotionPacket::from_object(&handle(packet)?)?,
            )));
        }
        if is::<mc_bindings::ClientboundPlayerInfoUpdatePacket>(packet)? {
            return Ok(Some(Packet::ClientboundPlayerInfoUpdate(
                ClientboundPlayerInfoUpdatePacket {
                    packet: handle(packet)?,
                },
            )));
        }
        Ok(None)
    }

    pub fn to_object(self) -> anyhow::Result<Object> {
        match self {
            Packet::ServerboundMovePlayer(packet) => packet.to_object(),
            Packet::ClientboundSetEntityMotion(packet) => Ok(packet.to_object()?),
            // Never mutated, so the original goes back.
            Packet::ClientboundPlayerInfoUpdate(packet) => Ok(packet.packet),
        }
    }
}
