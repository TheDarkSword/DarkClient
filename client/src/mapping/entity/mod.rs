use crate::mapping::{FieldType, GameContext, MinecraftClassType};
use jni::objects::{GlobalRef, JValue};

pub mod player;

#[derive(Debug, Clone)]
pub struct EntityLivingBase {
    pub jni_entity_living_base: GlobalRef,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub jni_entity: GlobalRef,
}

impl GameContext for Entity {}

impl Entity {
    pub fn new(jni_entity: GlobalRef) -> Entity {
        Entity { jni_entity }
    }

    pub fn get_position(&self) -> (f64, f64, f64) {
        let mapping = self.mapping();

        let vec3 = mapping
            .call_method(
                MinecraftClassType::Entity,
                self.jni_entity.as_obj(),
                "position",
                &[],
            )
            .l()
            .unwrap();

        let x = mapping
            .get_field(MinecraftClassType::Vec3, &vec3, "x", FieldType::Double)
            .d()
            .unwrap();

        let y = mapping
            .get_field(MinecraftClassType::Vec3, &vec3, "y", FieldType::Double)
            .d()
            .unwrap();

        let z = mapping
            .get_field(MinecraftClassType::Vec3, &vec3, "z", FieldType::Double)
            .d()
            .unwrap();

        (x, y, z)
    }

    pub fn set_invulnerable(&self, value: bool) {
        let mapping = self.mapping();

        mapping.call_method(
            MinecraftClassType::Entity,
            self.jni_entity.as_obj(),
            "setInvulnerable",
            &[JValue::from(value)],
        );
    }

    pub fn get_fall_distance(&self) -> f64 {
        let mapping = self.mapping();

        mapping
            .get_field(
                MinecraftClassType::Entity,
                self.jni_entity.as_obj(),
                "fallDistance",
                FieldType::Double,
            )
            .d()
            .unwrap()
    }

    pub fn reset_fall_distance(&self) {
        let mapping = self.mapping();

        mapping
            .call_method(
                MinecraftClassType::Entity,
                self.jni_entity.as_obj(),
                "resetFallDistance",
                &[],
            )
            .v()
            .unwrap();
    }

    pub fn get_name(&self) -> String {
        let mapping = self.mapping();

        mapping.get_string(
            mapping
                .call_method(
                    MinecraftClassType::Entity,
                    self.jni_entity.as_obj(),
                    "getName",
                    &[],
                )
                .l()
                .unwrap(),
        )
    }
}
