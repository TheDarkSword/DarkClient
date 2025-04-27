use jni::objects::{GlobalRef, JValue};
use jni::sys::jboolean;
use crate::LogExpect;
use crate::mapping::{FieldType, GameContext, Mapping, MinecraftClassType};
use crate::mapping::entity::Entity;

#[derive(Debug, Clone)]
pub struct LocalPlayer {
    pub jni_ref: GlobalRef,
    pub entity: Entity,
    //pub capabilities: PlayerCapabilities
}

#[derive(Debug, Clone)]
pub struct PlayerCapabilities {
    pub jni_ref: GlobalRef
}

impl GameContext for LocalPlayer {}

impl LocalPlayer {
    pub fn new(minecraft: &GlobalRef, mapping: &Mapping) -> Self {
        let player_obj = mapping.get_field(
            MinecraftClassType::Minecraft,
            minecraft.as_obj(),
            "player",
            FieldType::Object(MinecraftClassType::LocalPlayer, mapping)
        ).l().unwrap();
        
        let player_ref = mapping.new_global_ref(player_obj);
        
        let entity = Entity::new(player_ref.clone());

        //let jni_capabilities = mapping.get_field(
        //    MinecraftClassType::EntityPlayer,
        //    &player_obj,
        //    "capabilities",
        //    FieldType::Object(MinecraftClassType::PlayerCapabilities, mapping)
        //).l().unwrap();

        //let capabilities = PlayerCapabilities {
        //    jni_player_capabilities: mapping.new_global_ref(jni_capabilities)
        //};

        Self {
            jni_ref: player_ref,
            entity
            //capabilities
        }
    }

    //pub fn fly(&self, value: bool) {
    //    self.capabilities.fly(value);
    //}
}

impl GameContext for PlayerCapabilities {}

impl PlayerCapabilities {

    pub fn fly(&self, value: bool) {
        let client = self.client();
        let mut env = client.get_env().unwrap();

        let value: jboolean = if value {1} else {0};

        env.set_field(
            self.jni_ref.as_obj(),
            "b",
            "Z",
            JValue::Bool(value)
        ).log_expect("Error 1");
    }
}