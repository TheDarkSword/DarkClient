//! Shared game logic on top of the generated types, as extension traits.

pub mod prelude;
pub mod render;
pub mod screen;

use mc_bindings::{
    BarrelBlockEntity, BlockEntity, BlockHitResult, BlockPos, ChestBlockEntity, ClientLevel,
    Direction, Double, EnderChestBlockEntity, Entity, Integer, InteractionHand, JavaType,
    LivingEntity, LocalPlayer, MultiPlayerGameMode, Object, OptionInstance, Result,
    ShulkerBoxBlockEntity, Vec3,
};

pub const SLOT_OFFHAND: i32 = 40;

/// Inventory index → `InventoryMenu` slot: the hotbar (0-8) sits at 36-44.
pub fn menu_slot(inventory_slot: i32) -> i32 {
    if inventory_slot < 9 {
        inventory_slot + 36
    } else {
        inventory_slot
    }
}

pub trait ObjectExt {
    /// `false` also when the check itself fails.
    fn instance_of<T: JavaType>(&self) -> bool;
}

impl ObjectExt for Object {
    fn instance_of<T: JavaType>(&self) -> bool {
        self.is::<T>().unwrap_or(false)
    }
}

pub trait EntityExt {
    /// Minecraft renders at `lerp(partial_tick, old_position, position)`.
    fn old_position(&self) -> Result<Vec3>;

    /// Writes the previous-tick rotation too, so Minecraft doesn't interpolate
    /// on top of our own smoothing.
    fn set_rotation(&self, yaw: f32, pitch: f32) -> Result<()>;

    fn as_living(&self) -> Option<LivingEntity>;
}

impl EntityExt for Entity {
    fn old_position(&self) -> Result<Vec3> {
        Ok(Vec3::new(self.x_old()?, self.y_old()?, self.z_old()?))
    }

    fn set_rotation(&self, yaw: f32, pitch: f32) -> Result<()> {
        self.set_y_rot(yaw)?;
        self.set_x_rot(pitch)?;
        self.set_y_rot_o(yaw)?;
        self.set_x_rot_o(pitch)
    }

    fn as_living(&self) -> Option<LivingEntity> {
        self.cast().ok().flatten()
    }
}

pub trait LivingEntityExt {
    /// A player on the respawn screen is not alive.
    fn is_alive(&self) -> Result<bool>;

    /// Strafe (`xxa`) and forward (`zza`) input.
    fn move_input(&self) -> Result<(f32, f32)>;

    fn has_move_input(&self) -> Result<bool>;
}

impl LivingEntityExt for LivingEntity {
    fn is_alive(&self) -> Result<bool> {
        Ok(!self.is_dead_or_dying()?)
    }

    fn move_input(&self) -> Result<(f32, f32)> {
        Ok((self.xxa()?, self.zza()?))
    }

    fn has_move_input(&self) -> Result<bool> {
        let (strafe, forward) = self.move_input()?;
        Ok(strafe != 0.0 || forward != 0.0)
    }
}

pub trait ClientLevelExt {
    fn is_block_air(&self, pos: BlockPos) -> Result<bool>;
}

impl ClientLevelExt for ClientLevel {
    fn is_block_air(&self, pos: BlockPos) -> Result<bool> {
        self.get_block_state(pos)?.is_air()
    }
}

pub trait GameModeExt {
    /// Uses the main-hand item on `face` of `anchor`, as a right click at `hit`.
    fn place_block_on(
        &self,
        player: &LocalPlayer,
        anchor: BlockPos,
        face: Direction,
        hit: Vec3,
    ) -> Result<()>;
}

impl GameModeExt for MultiPlayerGameMode {
    fn place_block_on(
        &self,
        player: &LocalPlayer,
        anchor: BlockPos,
        face: Direction,
        hit: Vec3,
    ) -> Result<()> {
        let hit = BlockHitResult::new(hit, face, anchor, false)?;
        self.use_item_on(player, InteractionHand::MainHand, &hit)?;
        Ok(())
    }
}

pub trait BlockPosExt {
    fn offset(&self, dx: i32, dy: i32, dz: i32) -> BlockPos;
}

impl BlockPosExt for BlockPos {
    fn offset(&self, dx: i32, dy: i32, dz: i32) -> BlockPos {
        BlockPos::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

pub trait DirectionExt {
    fn offset(self) -> (i32, i32, i32);

    fn opposite(self) -> Direction;
}

impl DirectionExt for Direction {
    fn offset(self) -> (i32, i32, i32) {
        match self {
            Direction::Down => (0, -1, 0),
            Direction::Up => (0, 1, 0),
            Direction::North => (0, 0, -1),
            Direction::South => (0, 0, 1),
            Direction::West => (-1, 0, 0),
            Direction::East => (1, 0, 0),
        }
    }

    fn opposite(self) -> Direction {
        match self {
            Direction::Down => Direction::Up,
            Direction::Up => Direction::Down,
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }
}

pub trait OptionInstanceExt {
    fn get_int(&self) -> anyhow::Result<i32>;

    fn get_double(&self) -> anyhow::Result<f64>;

    /// Skips `OptionInstance.set`'s validation, so gamma can go past 1 (Fullbright).
    fn force_double(&self, value: f64) -> anyhow::Result<()>;
}

impl OptionInstanceExt for OptionInstance {
    fn get_int(&self) -> anyhow::Result<i32> {
        let value = self.get()?;
        let integer: Integer = value
            .cast()?
            .ok_or_else(|| anyhow::anyhow!("option value is not an Integer"))?;
        Ok(integer.int_value()?)
    }

    fn get_double(&self) -> anyhow::Result<f64> {
        let value = self.get()?;
        let double: Double = value
            .cast()?
            .ok_or_else(|| anyhow::anyhow!("option value is not a Double"))?;
        Ok(double.double_value()?)
    }

    fn force_double(&self, value: f64) -> anyhow::Result<()> {
        let boxed = Double::value_of(value)?;
        Ok(self.set_value(&boxed)?)
    }
}

pub trait BlockEntityExt {
    fn is_container(&self) -> bool;
}

impl BlockEntityExt for BlockEntity {
    fn is_container(&self) -> bool {
        self.instance_of::<ChestBlockEntity>()
            || self.instance_of::<EnderChestBlockEntity>()
            || self.instance_of::<BarrelBlockEntity>()
            || self.instance_of::<ShulkerBoxBlockEntity>()
    }
}
