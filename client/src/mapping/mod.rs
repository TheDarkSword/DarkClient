use jni::objects::{GlobalRef, JObject, JString, JValue, JValueOwned};
use serde::Deserialize;
use std::collections::HashMap;
use jni::JNIEnv;
use crate::client::DarkClient;
use crate::LogExpect;
use crate::mapping::client::minecraft::Minecraft;

pub mod client;
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

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

impl FieldType<'_> {

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

impl Mapping {
    pub fn new() -> Self {
        let contents = include_str!("../../../mappings.json");
        let mapping: Mapping = serde_json::from_str(contents).log_expect("Failed to parse mapping");
        mapping
    }

    fn get_client(&self) -> &DarkClient {
        DarkClient::instance()
    }

    fn get_env(&self) -> JNIEnv {
        self.get_client().get_env().log_expect("Failed to get jni env")
    }

    pub fn get_class(&self, name: &str) -> &MinecraftClass {
        self.classes.get(name).log_expect(format!("{} java class not found", name).as_str())
    }

    pub fn call_static_method(&self, class_type: MinecraftClassType, method_name: &str, args: &[JValue]) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let jclass = env.find_class(&class.name).log_expect(format!("{} class not found", class_type.get_name()).as_str());
        let method = class.get_method(method_name);
        env.call_static_method(
            jclass,
            &method.name,
            &method.signature,
            args
        ).log_expect(format!("Error when calling static method {} in class {} with method signature {}", method.name, class.name, method.signature).as_str())
    }

    pub fn call_method(&self, class_type: MinecraftClassType, instance: &JObject, method_name: &str, args: &[JValue]) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let method = class.get_method(method_name);
        env.call_method(
            instance,
            &method.name,
            &method.signature,
            args
        ).log_expect(format!("Error when calling method {} in class {} with method signature {}", method.name, class.name, method.signature).as_str())
    }

    pub fn get_static_field(&self, class_type: MinecraftClassType, field_name: &str, field_type: FieldType) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let jclass = env.find_class(&class.name).log_expect(format!("{} class not found", class_type.get_name()).as_str());
        let field = class.get_field(field_name);
        env.get_static_field(
            jclass,
            &field.name,
            field_type.get_signature()
        ).log_expect(format!("Error when getting static field {}", field.name).as_str())
    }

    pub fn get_field(&self, class_type: MinecraftClassType, instance: &JObject, field_name: &str, field_type: FieldType) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let field = class.get_field(field_name);

        env.get_field(
            instance,
            &field.name,
            field_type.get_signature()
        ).log_expect(format!("Error when getting field {}", field.name).as_str())
    }

    pub fn set_field(&self, class_type: MinecraftClassType, instance: &JObject, field_name: &str, field_type: FieldType, value: JValue) {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let field = class.get_field(field_name);
        env.set_field(
            instance,
            &field.name,
            field_type.get_signature(),
            value
        ).log_expect(format!("Error when setting field {}", field.name).as_str());
    }

    pub fn new_global_ref(&self, obj: JObject) -> GlobalRef {
        let env = self.get_env();
        env.new_global_ref(obj).unwrap()
    }

    pub fn get_string(&self, obj: JObject) -> String {
        let env = self.get_env();
        let jstring = JString::from(obj);
        unsafe { let value = env.get_string_unchecked(jstring.as_ref()).unwrap().to_str().unwrap().to_string(); value }
    }
}