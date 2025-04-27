use jni::objects::{GlobalRef};
use crate::mapping::{FieldType, GameContext, MinecraftClassType};

#[derive(Debug, Clone)]
pub struct Entity {
    pub jni_entity: GlobalRef
}

impl GameContext for Entity {}

impl Entity {
    pub fn new(jni_entity: GlobalRef) -> Entity {
        Entity {
            jni_entity
        }
    }

    pub fn get_position(&self) -> (f64, f64, f64) {
        let mapping = self.mapping();
        
        let vec3 = mapping.call_method(
            MinecraftClassType::Entity,
            self.jni_entity.as_obj(),
            "position",
            &[]
        ).l().unwrap();

        let x = mapping.get_field(
            MinecraftClassType::Vec3,
            &vec3,
            "x",
            FieldType::Double
        ).d().unwrap() as f64;

        let y = mapping.get_field(
            MinecraftClassType::Vec3,
            &vec3,
            "y",
            FieldType::Double
        ).d().unwrap() as f64;

        let z = mapping.get_field(
            MinecraftClassType::Vec3,
            &vec3,
            "z",
            FieldType::Double
        ).d().unwrap() as f64;

        (x, y, z)
    }

    pub fn get_name(&self) -> String {
        let mapping = self.mapping();

        mapping.get_string(mapping.call_method(
            MinecraftClassType::Entity,
            self.jni_entity.as_obj(),
            "getName",
            &[]
        ).l().unwrap())
    }
}