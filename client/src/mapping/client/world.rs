use crate::mapping::{FieldType, GameContext, Mapping, MinecraftClassType};
use jni::objects::GlobalRef;

#[derive(Debug)]
pub struct World {
    jni_world: GlobalRef,
}

impl GameContext for World {}

impl World {
    pub fn new(minecraft: &GlobalRef, mapping: &Mapping) -> World {
        let world_obj = mapping
            .get_field(
                MinecraftClassType::Minecraft,
                minecraft.as_obj(),
                "level",
                FieldType::Object(MinecraftClassType::Level, mapping),
            )
            .l()
            .unwrap();

        World {
            jni_world: mapping.new_global_ref(world_obj),
        }
    }
}
