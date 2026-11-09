//! Wrapper for Minecraft's `GameRenderer`.

use crate::mapping::client::camera::Camera;
use crate::mapping::{FieldType, MappedObject, MinecraftClassType as Cls};
use crate::state::mapping;
use jni::objects::GlobalRef;

/// Minecraft's `GameRenderer`.
#[derive(Debug, Clone, MappedObject)]
#[mapped(class = GameRenderer)]
pub struct GameRenderer {
    jni_ref: GlobalRef,
}

/// The inputs of `GameRenderer.bobHurt` / `bobView` for the frame just
/// rendered: the damage tilt and walking sway Minecraft folds into its
/// projection matrix. A value snapshot of the camera render state (26.1+).
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraBob {
    pub view_bobbing: bool,
    pub damage_tilt_strength: f32,
    pub is_player: bool,
    pub is_living: bool,
    pub is_dead_or_dying: bool,
    pub death_time: f32,
    pub hurt_time: f32,
    pub hurt_duration: f32,
    pub hurt_dir: f32,
    pub walk_distance: f32,
    pub bob: f32,
}

impl GameRenderer {
    /// Wraps an existing `GameRenderer` JVM object.
    pub fn new(jni_ref: GlobalRef) -> GameRenderer {
        GameRenderer { jni_ref }
    }

    /// The main render [`Camera`].
    pub fn get_main_camera(&self) -> anyhow::Result<Camera> {
        self.in_frame(|| {
            let camera = self.call_method("mainCamera", &[])?.l()?;
            Ok(Camera::new(mapping().new_global_ref(camera)?))
        })
    }

    /// Snapshots the view-bobbing and damage-tilt state of the last frame.
    /// Fails on builds without the render-state classes (before 26.1).
    pub fn camera_bob(&self) -> anyhow::Result<CameraBob> {
        self.in_frame(|| {
            let mapping = mapping();
            let object = |class, owner: &jni::objects::JObject, name, field_class| {
                mapping
                    .get_field(class, owner, name, FieldType::Object(field_class))?
                    .l()
                    .map_err(anyhow::Error::from)
            };
            let game = self
                .get_field("gameRenderState", FieldType::Object(Cls::GameRenderState))?
                .l()?;
            let options = object(
                Cls::GameRenderState,
                &game,
                "optionsRenderState",
                Cls::OptionsRenderState,
            )?;
            let level = object(
                Cls::GameRenderState,
                &game,
                "levelRenderState",
                Cls::LevelRenderState,
            )?;
            let camera = object(
                Cls::LevelRenderState,
                &level,
                "cameraRenderState",
                Cls::CameraRenderState,
            )?;
            let entity = object(
                Cls::CameraRenderState,
                &camera,
                "entityRenderState",
                Cls::CameraEntityRenderState,
            )?;

            let float = |name| -> anyhow::Result<f32> {
                Ok(mapping
                    .get_field(
                        Cls::CameraEntityRenderState,
                        &entity,
                        name,
                        FieldType::Float,
                    )?
                    .f()?)
            };
            let flag = |name| -> anyhow::Result<bool> {
                Ok(mapping
                    .get_field(
                        Cls::CameraEntityRenderState,
                        &entity,
                        name,
                        FieldType::Boolean,
                    )?
                    .z()?)
            };
            Ok(CameraBob {
                view_bobbing: mapping
                    .get_field(
                        Cls::OptionsRenderState,
                        &options,
                        "bobView",
                        FieldType::Boolean,
                    )?
                    .z()?,
                damage_tilt_strength: mapping
                    .get_field(
                        Cls::OptionsRenderState,
                        &options,
                        "damageTiltStrength",
                        FieldType::Double,
                    )?
                    .d()? as f32,
                is_player: flag("isPlayer")?,
                is_living: flag("isLiving")?,
                is_dead_or_dying: flag("isDeadOrDying")?,
                death_time: float("deathTime")?,
                hurt_time: float("hurtTime")?,
                hurt_duration: mapping
                    .get_field(
                        Cls::CameraEntityRenderState,
                        &entity,
                        "hurtDuration",
                        FieldType::Int,
                    )?
                    .i()? as f32,
                hurt_dir: float("hurtDir")?,
                walk_distance: float("backwardsInterpolatedWalkDistance")?,
                bob: float("bob")?,
            })
        })
    }
}
