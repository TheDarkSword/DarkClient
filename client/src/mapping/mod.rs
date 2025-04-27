use jni::objects::{GlobalRef, JValueOwned};
use serde::Deserialize;
use std::collections::HashMap;
use crate::client::DarkClient;
use crate::LogExpect;
use crate::mapping::client::minecraft::Minecraft;

pub mod client;
pub mod mapping;
pub mod entity;
pub mod java;

pub trait GameContext {
    fn client(&self) -> &'static DarkClient {
        DarkClient::instance()
    }

    fn minecraft(&self) -> &'static Minecraft {
        Minecraft::instance()
    }

    fn mapping(&self) -> &'static Mapping {
        self.minecraft().get_mapping()
    }
}

#[derive(Debug, Deserialize)]
pub struct Mapping {
    classes: HashMap<String, MinecraftClass>
}

#[derive(Debug, Deserialize)]
pub struct MinecraftClass {
    name: String,
    methods: HashMap<String, Method>,
    fields: HashMap<String, Field>
}

#[derive(Debug, Deserialize)]
pub struct Method {
    name: String,
    signature: String
}

#[derive(Debug, Deserialize)]
pub struct Field {
    name: String
}

impl MinecraftClass {

    pub fn get_method(&self, name: &str) -> &Method {
        self.methods.get(name).log_expect(format!("{} method not found", name).as_str())
    }

    pub fn get_field(&self, name: &str) -> &Field {
        self.fields.get(name).log_expect(format!("{} field not found", name).as_str())
    }
}

pub enum MinecraftClassType {
    Minecraft,
    LocalPlayer,
    Level,
    PlayerCapabilities,
    EntityPlayer,
    Entity,
    Vec3,
    Window,
}

impl MinecraftClassType {

    pub fn get_name(&self) -> &str {
        match self {
            MinecraftClassType::Minecraft => "net/minecraft/client/Minecraft",
            MinecraftClassType::LocalPlayer => "net/minecraft/client/player/LocalPlayer",
            MinecraftClassType::Level => "net/minecraft/client/multiplayer/ClientLevel",
            MinecraftClassType::PlayerCapabilities => "net/minecraft/entity/entity/PlayerCapabilities",
            MinecraftClassType::EntityPlayer => "net/minecraft/entity/entity/EntityPlayer",
            MinecraftClassType::Entity => "net/minecraft/world/entity/Entity",
            MinecraftClassType::Vec3 => "net/minecraft/world/phys/Vec3",
            MinecraftClassType::Window => "com/mojang/blaze3d/platform/Window",
        }
    }
}

#[allow(dead_code)]
pub enum FieldType<'local> {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    String,
    Object(MinecraftClassType, &'local Mapping)
}

impl<'local> FieldType<'local> {

    pub fn get_signature(&self) -> String {
        match self {
            FieldType::Boolean => String::from("Z"),
            FieldType::Byte => String::from("B"),
            FieldType::Char => String::from("C"),
            FieldType::Short => String::from("S"),
            FieldType::Int => String::from("I"),
            FieldType::Long => String::from("J"),
            FieldType::Float => String::from("F"),
            FieldType::Double => String::from("D"),
            FieldType::String => String::from("Ljava/lang/String;"),
            FieldType::Object(minecraft_class_type, mapping) => {
                let class_name = &mapping.get_class(minecraft_class_type.get_name()).name;
                format!("L{};", class_name)
            }
        }
    }
}