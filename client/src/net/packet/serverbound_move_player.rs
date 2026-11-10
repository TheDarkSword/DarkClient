//! The packet is abstract: `Pos`, `PosRot`, `Rot` or `StatusOnly`, depending on
//! what it carries, so it's rebuilt as the matching subclass.

use mc_bindings::{
    JavaClass, MovePlayerPos, MovePlayerPosRot, MovePlayerRot, MovePlayerStatusOnly, Object,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerboundMovePlayerPacket {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub has_position: bool,
    pub has_rotation: bool,
}

impl ServerboundMovePlayerPacket {
    pub fn read(
        packet: &mc_bindings::ServerboundMovePlayerPacket,
    ) -> anyhow::Result<ServerboundMovePlayerPacket> {
        Ok(ServerboundMovePlayerPacket {
            x: packet.get_x(0.0)?,
            y: packet.get_y(0.0)?,
            z: packet.get_z(0.0)?,
            y_rot: packet.get_y_rot(0.0)?,
            x_rot: packet.get_x_rot(0.0)?,
            on_ground: packet.is_on_ground()?,
            horizontal_collision: packet.horizontal_collision()?,
            has_position: packet.has_position()?,
            has_rotation: packet.has_rotation()?,
        })
    }

    pub fn to_object(self) -> anyhow::Result<Object> {
        let (on_ground, collision) = (self.on_ground, self.horizontal_collision);
        let packet = match (self.has_position, self.has_rotation) {
            (true, true) => MovePlayerPosRot::new(
                self.x, self.y, self.z, self.y_rot, self.x_rot, on_ground, collision,
            )?
            .as_object()
            .clone(),
            (true, false) => MovePlayerPos::new(self.x, self.y, self.z, on_ground, collision)?
                .as_object()
                .clone(),
            (false, true) => MovePlayerRot::new(self.y_rot, self.x_rot, on_ground, collision)?
                .as_object()
                .clone(),
            (false, false) => MovePlayerStatusOnly::new(on_ground, collision)?
                .as_object()
                .clone(),
        };
        Ok(packet)
    }
}
