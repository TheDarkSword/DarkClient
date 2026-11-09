//! The entity wrappers, mirroring Minecraft's `Entity → LivingEntity → Player`
//! class hierarchy.
//!
//! Rust has no inheritance, so the hierarchy is expressed with **traits**: each
//! level is a trait that extends the one below — `PlayerRef: LivingEntityRef:
//! EntityRef`. A wrapper struct implements the trait for its own level and
//! every level beneath it, so a `Player` value can call `Entity`,
//! `LivingEntity` and `Player` methods alike, with no nested-field hops.
//!
//! The trait methods are *provided* (default) methods: each calls
//! `self.call_method(...)`, which reflects against the **wrapper's own**
//! Minecraft class — and reflection resolves inherited methods, so an inherited
//! call resolves correctly on a subclass wrapper. The whole scheme is static
//! dispatch: zero-cost, monomorphized per wrapper type.

use crate::mapping::component::Component;
use crate::mapping::entity::living::LivingEntity;
use crate::mapping::math::Vec3;
use crate::mapping::minecraft_version::MinecraftVersion;
use crate::mapping::{FieldType, MappedObject, MinecraftClassType};
use crate::state::mapping;
use jni::objects::{GlobalRef, JValue};

pub mod living;
pub mod mob;
pub mod player;

/// A plain Minecraft `Entity` — the base wrapper. Subclass wrappers
/// (`LivingEntity`, `Mob`, `Player`, `LocalPlayer`) get the same methods
/// through [`EntityRef`].
#[derive(Debug, Clone, MappedObject)]
#[mapped(class = Entity)]
pub struct Entity {
    jni_ref: GlobalRef,
}

impl Entity {
    /// Wraps an existing `Entity` JVM object.
    pub fn new(jni_ref: GlobalRef) -> Entity {
        Entity { jni_ref }
    }
}

impl EntityRef for Entity {}

/// The behaviour of a Minecraft `Entity`. Implemented by every entity wrapper —
/// they all hold a live JVM object, and reflection resolves inherited methods,
/// so these calls work on a subclass wrapper just as well.
pub trait EntityRef: MappedObject + Sized {
    /// Views this entity as a [`LivingEntity`], or `None` when it is not a
    /// living entity — calling health methods on a non-living entity would fail.
    fn as_living(&self) -> Option<LivingEntity> {
        self.instance_of::<LivingEntity>()
            .then(|| LivingEntity::new(self.jni_ref().clone()))
    }

    /// The entity's network id.
    fn id(&self) -> anyhow::Result<i32> {
        Ok(self.call_method("getId", &[])?.i()?)
    }

    /// The entity's world position (feet).
    fn get_position(&self) -> anyhow::Result<Vec3> {
        self.in_frame(|| {
            let vec3 = self.call_method("position", &[])?.l()?;
            Vec3::read(&vec3)
        })
    }

    /// The entity's position at the previous tick — Minecraft renders it at
    /// `lerp(partial_tick, old_position, position)`.
    fn get_old_position(&self) -> anyhow::Result<Vec3> {
        Ok(Vec3::new(
            self.get_field("xOld", FieldType::Double)?.d()?,
            self.get_field("yOld", FieldType::Double)?.d()?,
            self.get_field("zOld", FieldType::Double)?.d()?,
        ))
    }

    /// The entity's eye position — the origin to aim rotations from.
    fn get_eye_position(&self) -> anyhow::Result<Vec3> {
        self.in_frame(|| {
            let vec3 = self.call_method("getEyePosition", &[])?.l()?;
            Vec3::read(&vec3)
        })
    }

    /// The entity's yaw, in degrees.
    fn get_yaw(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getYRot", &[])?.f()?)
    }

    /// The entity's pitch, in degrees.
    fn get_pitch(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getXRot", &[])?.f()?)
    }

    /// Squared distance from this entity to a world point.
    fn distance_to_sqr(&self, x: f64, y: f64, z: f64) -> anyhow::Result<f64> {
        Ok(self
            .call_method(
                "distanceToSqr",
                &[JValue::Double(x), JValue::Double(y), JValue::Double(z)],
            )?
            .d()?)
    }

    /// Collision-box width.
    fn bb_width(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getBbWidth", &[])?.f()?)
    }

    /// Collision-box height.
    fn bb_height(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getBbHeight", &[])?.f()?)
    }

    /// Whether the entity is currently sprinting.
    fn is_sprinting(&self) -> anyhow::Result<bool> {
        Ok(self.call_method("isSprinting", &[])?.z()?)
    }

    /// Sets the entity's sprinting flag.
    fn set_sprinting(&self, value: bool) -> anyhow::Result<()> {
        self.call_method("setSprinting", &[JValue::from(value)])?;
        Ok(())
    }

    /// Whether the entity is currently standing on the ground.
    fn on_ground(&self) -> anyhow::Result<bool> {
        Ok(self.call_method("onGround", &[])?.z()?)
    }

    /// The entity's current velocity.
    fn get_delta_movement(&self) -> anyhow::Result<Vec3> {
        self.in_frame(|| {
            let vec3 = self.call_method("getDeltaMovement", &[])?.l()?;
            Vec3::read(&vec3)
        })
    }

    /// Sets the entity's velocity.
    fn set_delta_movement(&self, x: f64, y: f64, z: f64) -> anyhow::Result<()> {
        self.call_method(
            "setDeltaMovement",
            &[JValue::Double(x), JValue::Double(y), JValue::Double(z)],
        )?;
        Ok(())
    }

    /// Sets the entity's position.
    fn set_pos(&self, x: f64, y: f64, z: f64) -> anyhow::Result<()> {
        self.call_method(
            "setPos",
            &[JValue::Double(x), JValue::Double(y), JValue::Double(z)],
        )?;
        Ok(())
    }

    /// Whether the entity is sneaking (the shift key is held).
    fn is_shift_key_down(&self) -> anyhow::Result<bool> {
        Ok(self.call_method("isShiftKeyDown", &[])?.z()?)
    }

    /// Sets the entity's `noPhysics` flag — when set, the entity ignores block
    /// collision (no clip).
    fn set_no_physics(&self, value: bool) -> anyhow::Result<()> {
        self.set_field("noPhysics", FieldType::Boolean, JValue::from(value))?;
        Ok(())
    }

    /// Sets the entity's invulnerability flag.
    fn set_invulnerable(&self, value: bool) -> anyhow::Result<()> {
        // Inherited by every entity wrapper, so not expressible in the
        // class-keyed rename registry.
        let setter = if mapping().get_version() >= MinecraftVersion::new(26, 3, 0) {
            "setPermanentlyInvulnerable"
        } else {
            "setInvulnerable"
        };
        self.call_method(setter, &[JValue::from(value)])?;
        Ok(())
    }

    /// Sets the entity's yaw and pitch, in degrees. The previous-tick rotation
    /// (`yRotO` / `xRotO`) is written too, so Minecraft renders the camera
    /// exactly at this rotation instead of interpolating toward it — the
    /// per-frame rotation system supplies the smoothing itself.
    fn set_rotation(&self, yaw: f32, pitch: f32) -> anyhow::Result<()> {
        self.call_method("setYRot", &[JValue::Float(yaw)])?;
        self.call_method("setXRot", &[JValue::Float(pitch)])?;
        self.set_field("yRotO", FieldType::Float, JValue::Float(yaw))?;
        self.set_field("xRotO", FieldType::Float, JValue::Float(pitch))?;
        Ok(())
    }

    /// The accumulated fall distance.
    fn get_fall_distance(&self) -> anyhow::Result<f64> {
        Ok(self.get_field("fallDistance", FieldType::Double)?.d()?)
    }

    /// Resets the accumulated fall distance to zero.
    fn reset_fall_distance(&self) -> anyhow::Result<()> {
        Ok(self.call_method("resetFallDistance", &[])?.v()?)
    }

    /// The entity's display name, as a [`Component`].
    fn get_name(&self) -> anyhow::Result<Component> {
        self.in_frame(|| {
            let component = self.call_method("getName", &[])?.l()?;
            Ok(Component::new(mapping().new_global_ref(component)?))
        })
    }

    /// The entity's age in ticks since it spawned.
    fn get_tick_count(&self) -> anyhow::Result<i32> {
        Ok(self.get_field("tickCount", FieldType::Int)?.i()?)
    }
}

/// The behaviour of a Minecraft `LivingEntity` — an [`EntityRef`] that also
/// carries health and can swing its arm.
pub trait LivingEntityRef: EntityRef {
    /// Current health.
    fn get_health(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getHealth", &[])?.f()?)
    }

    /// Maximum health.
    fn get_max_health(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getMaxHealth", &[])?.f()?)
    }

    /// Whether the entity is alive — *not* dead and not playing its death
    /// animation (`!isDeadOrDying()`). A player sitting on the respawn screen
    /// is **not** alive by this measure.
    fn is_alive(&self) -> anyhow::Result<bool> {
        Ok(!self.call_method("isDeadOrDying", &[])?.z()?)
    }

    /// Whether the entity is currently using an item — eating, drawing a bow,
    /// blocking with a shield, and so on.
    fn is_using_item(&self) -> anyhow::Result<bool> {
        Ok(self.call_method("isUsingItem", &[])?.z()?)
    }

    /// The entity's movement input this tick — the strafe (`xxa`) and forward
    /// (`zza`) impulse, in the entity's own local axes.
    fn move_input(&self) -> anyhow::Result<(f32, f32)> {
        let strafe = self.get_field("xxa", FieldType::Float)?.f()?;
        let forward = self.get_field("zza", FieldType::Float)?.f()?;
        Ok((strafe, forward))
    }

    /// Whether the entity currently has movement input — a non-zero strafe
    /// (`xxa`) or forward (`zza`) impulse, i.e. a movement key is held.
    fn has_move_input(&self) -> anyhow::Result<bool> {
        let (strafe, forward) = self.move_input()?;
        Ok(strafe != 0.0 || forward != 0.0)
    }

    /// The entity's movement speed (`getSpeed`) — the attribute-derived
    /// per-tick speed, including sprint and potion modifiers.
    fn get_speed(&self) -> anyhow::Result<f32> {
        Ok(self.call_method("getSpeed", &[])?.f()?)
    }

    /// Whether the entity currently holds the jump input.
    fn is_jumping(&self) -> anyhow::Result<bool> {
        Ok(self.get_field("jumping", FieldType::Boolean)?.z()?)
    }

    /// Plays the main-hand swing animation (and sends it to the server).
    fn swing(&self) -> anyhow::Result<()> {
        self.in_frame(|| {
            let hand = mapping()
                .get_static_field(
                    MinecraftClassType::InteractionHand,
                    "MAIN_HAND",
                    FieldType::Object(MinecraftClassType::InteractionHand),
                )?
                .l()?;
            if mapping().get_version() >= MinecraftVersion::new(26, 3, 0) {
                // 26.3 swings with the held item's attack animation, as
                // `Minecraft.startAttack` does.
                let stack = self
                    .call_method("getItemInHand", &[JValue::Object(&hand)])?
                    .l()?;
                let animation = mapping()
                    .call_method(
                        MinecraftClassType::ItemStack,
                        &stack,
                        "getAttackAnimation",
                        &[],
                    )?
                    .l()?;
                self.call_method(
                    "swing",
                    &[
                        JValue::Object(&hand),
                        JValue::Object(&animation),
                        JValue::from(false),
                    ],
                )?;
            } else {
                self.call_method("swing", &[JValue::Object(&hand)])?;
            }
            Ok(())
        })
    }
}

/// The behaviour of a Minecraft `Player` — a [`LivingEntityRef`] with the
/// melee attack cooldown.
pub trait PlayerRef: LivingEntityRef {
    /// The melee attack strength, `0.0..=1.0` — `1.0` once the attack cooldown
    /// has fully recharged. Below `1.0` the next hit deals reduced damage.
    fn attack_strength_scale(&self) -> anyhow::Result<f32> {
        Ok(self
            .call_method("getAttackStrengthScale", &[JValue::Float(0.5)])?
            .f()?)
    }
}
