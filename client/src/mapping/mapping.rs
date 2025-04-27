use jni::JNIEnv;
use crate::client::DarkClient;
use crate::mapping::*;
use crate::LogExpect;
use jni::objects::{JObject, JString, JValue};

impl Mapping {
    pub fn new() -> Self {
        let contents = include_str!("../../../mappings.json");
        let mapping: Mapping = serde_json::from_str(contents).log_expect("Failed to parse mappings");
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