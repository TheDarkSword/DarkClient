use crate::client::DarkClient;
use crate::mapping::client::minecraft::Minecraft;
use crate::LogExpect;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue, JValueOwned};
use jni::JNIEnv;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use serde::de::{MapAccess, Visitor};

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

/// Custom deserializer that handles both single Method and Vec<Method> formats
fn deserialize_methods<'de, D>(deserializer: D) -> Result<HashMap<String, Vec<Method>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct MethodsVisitor;

    impl<'de> Visitor<'de> for MethodsVisitor {
        type Value = HashMap<String, Vec<Method>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map of method names to methods or arrays of methods")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut methods = HashMap::new();

            while let Some(key) = map.next_key::<String>()? {
                // Try to deserialize as a single Method first
                match map.next_value::<MethodOrVec>()? {
                    MethodOrVec::Single(method) => {
                        methods.insert(key, vec![method]);
                    }
                    MethodOrVec::Multiple(method_vec) => {
                        methods.insert(key, method_vec);
                    }
                }
            }

            Ok(methods)
        }
    }

    deserializer.deserialize_map(MethodsVisitor)
}

/// Helper enum for deserializing either a single Method or Vec<Method>
#[derive(Deserialize)]
#[serde(untagged)]
enum MethodOrVec {
    Single(Method),
    Multiple(Vec<Method>),
}

/// Root structure containing all mapped Minecraft classes
#[derive(Debug, Deserialize)]
pub struct Mapping {
    classes: HashMap<String, MinecraftClass>,
}

/// Represents a mapped Minecraft class with its methods and fields
#[derive(Debug, Deserialize)]
pub struct MinecraftClass {
    name: String,
    #[serde(deserialize_with = "deserialize_methods", default)]
    methods: HashMap<String, Vec<Method>>,
    fields: HashMap<String, Field>,
}

/// Represents a method with its obfuscated name and JNI signature
#[derive(Debug, Deserialize)]
pub struct Method {
    name: String,
    signature: String,
}

/// Represents a field with its obfuscated name
#[derive(Debug, Deserialize)]
pub struct Field {
    name: String,
}

/// Signature matching result for method resolution
#[derive(Debug, PartialEq)]
enum SignatureMatch {
    Exact,
    Compatible,
    Incompatible,
}

impl MinecraftClass {
    pub fn get_method(&self, name: &str) -> &Method {
        self.methods
            .get(name)
            .unwrap().first()
            .log_expect(format!("{} method not found", name).as_str())
    }
    
    pub fn get_methods(&self, name: &str) -> &Vec<Method> {
        self.methods
            .get(name)
            .log_expect(format!("{} method not found", name).as_str())
    }

    pub fn get_method_by_signature(&self, name: &str, signature: &str) -> &Method {
        let methods = self.get_methods(name);
        methods
            .iter()
            .find(|method| method.signature == signature)
            .log_expect(format!("{} method with signature {} not found", name, signature).as_str())
    }

    pub fn get_method_by_args(&self, name: &str, args: &[JValue]) -> &Method {
        let methods = self.get_methods(name);

        // If only one method exists, return it immediately
        if methods.len() == 1 {
            return &methods[0];
        }

        // Find the best matching method based on argument compatibility
        let mut best_method = None;
        let mut best_match_quality = SignatureMatch::Incompatible;

        for method in methods {
            let match_quality = self.evaluate_signature_compatibility(&method.signature, args);

            if match_quality == SignatureMatch::Exact {
                // Exact match found, return immediately
                return method;
            }

            if match_quality == SignatureMatch::Compatible && best_match_quality != SignatureMatch::Exact {
                best_method = Some(method);
                best_match_quality = match_quality;
            }
        }

        match best_method {
            Some(method) => {
                log::debug!("Using compatible method '{}' with signature '{}' for args", name, method.signature);
                method
            }
            None => {
                log::warn!(
                    "No compatible method found for '{}' with {} arguments, using first available method",
                    name, args.len()
                );
                &methods[0]
            }
        }
    }

    /// Evaluates how well a method signature matches the provided arguments
    fn evaluate_signature_compatibility(&self, method_signature: &str, args: &[JValue]) -> SignatureMatch {
        let param_types = match self.extract_parameter_types(method_signature) {
            Ok(types) => types,
            Err(_) => return SignatureMatch::Incompatible,
        };

        // Check parameter count match
        if param_types.len() != args.len() {
            return SignatureMatch::Incompatible;
        }

        let mut exact_matches = 0;
        let mut compatible_matches = 0;

        // Check each parameter for compatibility
        for (param_type, arg) in param_types.iter().zip(args.iter()) {
            match self.check_type_compatibility(param_type, arg) {
                SignatureMatch::Exact => exact_matches += 1,
                SignatureMatch::Compatible => compatible_matches += 1,
                SignatureMatch::Incompatible => return SignatureMatch::Incompatible,
            }
        }

        if exact_matches == args.len() {
            SignatureMatch::Exact
        } else if exact_matches + compatible_matches == args.len() {
            SignatureMatch::Compatible
        } else {
            SignatureMatch::Incompatible
        }
    }

    /// Extracts parameter types from a JNI method signature
    ///
    /// # Example
    /// `(ILjava/lang/String;)V` -> `["I", "Ljava/lang/String;"]`
    fn extract_parameter_types(&self, signature: &str) -> Result<Vec<String>, &'static str> {
        let start = signature.find('(').ok_or("Invalid signature: missing opening parenthesis")?;
        let end = signature.find(')').ok_or("Invalid signature: missing closing parenthesis")?;

        if start >= end {
            return Err("Invalid signature: malformed parentheses");
        }

        let params_str = &signature[start + 1..end];
        if params_str.is_empty() {
            return Ok(Vec::new());
        }

        let mut types = Vec::new();
        let mut chars = params_str.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                // Primitive types
                'Z' | 'B' | 'C' | 'S' | 'I' | 'J' | 'F' | 'D' => {
                    types.push(ch.to_string());
                }
                // Object types
                'L' => {
                    let mut object_type = String::from("L");
                    while let Some(ch) = chars.next() {
                        object_type.push(ch);
                        if ch == ';' {
                            break;
                        }
                    }
                    types.push(object_type);
                }
                // Array types
                '[' => {
                    let mut array_type = String::from("[");
                    if let Some(&next_ch) = chars.peek() {
                        match next_ch {
                            'Z' | 'B' | 'C' | 'S' | 'I' | 'J' | 'F' | 'D' => {
                                array_type.push(chars.next().unwrap());
                            }
                            'L' => {
                                while let Some(ch) = chars.next() {
                                    array_type.push(ch);
                                    if ch == ';' {
                                        break;
                                    }
                                }
                            }
                            _ => return Err("Invalid array type in signature"),
                        }
                    }
                    types.push(array_type);
                }
                _ => return Err("Unknown type character in signature"),
            }
        }

        Ok(types)
    }

    /// Checks type compatibility between a JNI type signature and a JValue
    fn check_type_compatibility(&self, jni_type: &str, value: &JValue) -> SignatureMatch {
        match (jni_type, value) {
            // Exact primitive matches
            ("Z", JValue::Bool(_)) => SignatureMatch::Exact,
            ("B", JValue::Byte(_)) => SignatureMatch::Exact,
            ("C", JValue::Char(_)) => SignatureMatch::Exact,
            ("S", JValue::Short(_)) => SignatureMatch::Exact,
            ("I", JValue::Int(_)) => SignatureMatch::Exact,
            ("J", JValue::Long(_)) => SignatureMatch::Exact,
            ("F", JValue::Float(_)) => SignatureMatch::Exact,
            ("D", JValue::Double(_)) => SignatureMatch::Exact,

            // Numeric type promotions (compatible matches)
            ("I", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_)) => SignatureMatch::Compatible,
            ("J", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_) | JValue::Int(_)) => SignatureMatch::Compatible,
            ("F", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_) | JValue::Int(_)) => SignatureMatch::Compatible,
            ("D", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_) | JValue::Int(_) | JValue::Long(_) | JValue::Float(_)) => SignatureMatch::Compatible,

            // Object types - with proper type checking
            (jni_type, JValue::Object(obj)) if jni_type.starts_with('L') && jni_type.ends_with(';') => unsafe {
                self.check_object_type_compatibility(jni_type, obj)
            }

            // Arrays
            (jni_type, JValue::Object(obj)) if jni_type.starts_with('[') => unsafe {
                self.check_array_type_compatibility(jni_type, obj)
            }

            // Null handling - null can be assigned to any object type
            (jni_type, JValue::Object(obj)) if jni_type.starts_with('L') || jni_type.starts_with('[') => {
                if obj.is_null() {
                    SignatureMatch::Compatible
                } else {
                    SignatureMatch::Incompatible
                }
            }

            _ => SignatureMatch::Incompatible,
        }
    }

    /// Checks if an object matches the expected JNI object type signature
    unsafe fn check_object_type_compatibility(&self, expected_type: &str, obj: &JObject) -> SignatureMatch {
        // Handle null objects - they're compatible with any object type
        if obj.is_null() {
            return SignatureMatch::Compatible;
        }

        // Get the actual class name from the JNI type signature
        // Convert "Ljava/lang/String;" to "java/lang/String"
        let expected_class_name = &expected_type[1..expected_type.len()-1];

        // Special case for java.lang.Object - everything is compatible
        if expected_class_name == "java/lang/Object" {
            return SignatureMatch::Compatible;
        }

        // Get JNI environment to check actual object type
        if let Ok(mut env) = DarkClient::instance().get_env() {
            // Get the actual class of the object
            if let Ok(obj_class) = env.get_object_class(obj) {
                // Check for exact class match first
                if let Ok(expected_class) = env.find_class(expected_class_name) {
                    if let Ok(same_class) = env.is_same_object(&obj_class, &expected_class) {
                        if same_class {
                            return SignatureMatch::Exact;
                        }
                    }

                    // Check if the object is an instance of the expected type (inheritance/interface)
                    if let Ok(is_instance) = env.is_instance_of(obj, &expected_class) {
                        if is_instance {
                            return SignatureMatch::Compatible;
                        }
                    }
                }

                // Additional check for common Java types that might have special handling
                if let Ok(class_name) = self.get_class_name_from_object(&mut env, &obj_class) {
                    if self.are_compatible_types(&class_name, expected_class_name) {
                        return SignatureMatch::Compatible;
                    }
                }
            }
        }

        SignatureMatch::Incompatible
    }

    /// Checks if an array object matches the expected JNI array type signature
    unsafe fn check_array_type_compatibility(&self, expected_type: &str, obj: &JObject) -> SignatureMatch {
        // Handle null arrays
        if obj.is_null() {
            return SignatureMatch::Compatible;
        }

        if let Ok(mut env) = DarkClient::instance().get_env() {
            // Check if the object is actually an array
            if let Ok(obj_class) = env.get_object_class(obj) {
                if let Ok(class_name) = self.get_class_name_from_object(&mut env, &obj_class) {
                    // Array class names start with '['
                    if class_name.starts_with('[') {
                        // For exact match, the signatures should be identical
                        if class_name == expected_type {
                            return SignatureMatch::Exact;
                        }

                        // For compatible match, check if array types are compatible
                        // This is a simplified check - could be enhanced for inheritance
                        if self.are_compatible_array_types(&class_name, expected_type) {
                            return SignatureMatch::Compatible;
                        }
                    }
                }
            }
        }

        SignatureMatch::Incompatible
    }

    /// Gets the class name from a JClass object
    unsafe fn get_class_name_from_object(&self, env: &mut JNIEnv, class: &JClass) -> Result<String, Box<dyn std::error::Error>> {
        // Get the Class.getName() method to retrieve the class name
        let class_class = env.find_class("java/lang/Class")?;
        let get_name_method = env.get_method_id(&class_class, "getName", "()Ljava/lang/String;")?;

        // Call getName() on the class object
        let name_obj = env.call_method_unchecked(
            class,
            get_name_method,
            jni::signature::ReturnType::Object,
            &[]
        )?;

        if let JValueOwned::Object(name_str) = name_obj {
            let jstring = JString::from(name_str);
            let class_name = env.get_string(&jstring)?.to_str()?.to_string();

            // Convert Java class name format to JNI format
            // "java.lang.String" -> "java/lang/String"
            // "[Ljava.lang.String;" -> "[Ljava/lang/String;"
            let jni_name = class_name.replace('.', "/");
            Ok(jni_name)
        } else {
            Err("Failed to get class name as string".into())
        }
    }

    /// Checks if two class types are compatible (considering inheritance and common conversions)
    fn are_compatible_types(&self, actual_type: &str, expected_type: &str) -> bool {
        // Exact match
        if actual_type == expected_type {
            return true;
        }

        // Common Java type compatibility checks
        match (actual_type, expected_type) {
            // String and CharSequence compatibility
            ("java/lang/String", "java/lang/CharSequence") => true,

            // Wrapper types and their primitives are handled elsewhere
            // but we can add some common object conversions here

            // Collection hierarchy examples (extend as needed)
            ("java/util/ArrayList", "java/util/List") => true,
            ("java/util/LinkedList", "java/util/List") => true,
            ("java/util/HashSet", "java/util/Set") => true,
            ("java/util/HashMap", "java/util/Map") => true,

            // Common Minecraft type hierarchies (add your specific types here)
            _ => false,
        }
    }

    /// Checks if two array types are compatible
    fn are_compatible_array_types(&self, actual_type: &str, expected_type: &str) -> bool {
        // Extract the component types from both array signatures
        if let (Some(actual_component), Some(expected_component)) =
            (self.extract_array_component_type(actual_type), self.extract_array_component_type(expected_type)) {

            // For primitive arrays, they must match exactly
            if actual_component.len() == 1 && expected_component.len() == 1 {
                return actual_component == expected_component;
            }

            // For object arrays, check object compatibility
            if actual_component.starts_with('L') && expected_component.starts_with('L') {
                let actual_class = &actual_component[1..actual_component.len()-1];
                let expected_class = &expected_component[1..expected_component.len()-1];
                return self.are_compatible_types(actual_class, expected_class);
            }
        }

        false
    }

    /// Extracts the component type from an array type signature
    /// "[I" -> "I", "[Ljava/lang/String;" -> "Ljava/lang/String;"
    fn extract_array_component_type(&self, array_type: &str) -> Option<String> {
        if array_type.starts_with('[') && array_type.len() > 1 {
            Some(array_type[1..].to_string())
        } else {
            None
        }
    }

    pub fn get_field(&self, name: &str) -> &Field {
        self.fields
            .get(name)
            .log_expect(format!("{} field not found", name).as_str())
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MinecraftClassType {
    Minecraft,
    LocalPlayer,
    Level,
    Player,
    Abilities,
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
            MinecraftClassType::Player => "net/minecraft/world/entity/player/Player",
            MinecraftClassType::Abilities => "net/minecraft/world/entity/player/Abilities",
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
    Object(MinecraftClassType, &'local Mapping),
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
        self.get_client()
            .get_env()
            .log_expect("Failed to get jni env")
    }

    pub fn get_class(&self, name: &str) -> &MinecraftClass {
        self.classes
            .get(name)
            .log_expect(format!("{} java class not found", name).as_str())
    }

    pub fn call_static_method(
        &self,
        class_type: MinecraftClassType,
        method_name: &str,
        args: &[JValue],
    ) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let jclass = env
            .find_class(&class.name)
            .log_expect(format!("{} class not found", class_type.get_name()).as_str());
        let method = class.get_method_by_args(method_name, args);
        env.call_static_method(jclass, &method.name, &method.signature, args)
            .log_expect(
                format!(
                    "Error when calling static method {} in class {} with method signature {}",
                    method.name, class.name, method.signature
                )
                .as_str(),
            )
    }

    pub fn call_method(
        &self,
        class_type: MinecraftClassType,
        instance: &JObject,
        method_name: &str,
        args: &[JValue],
    ) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let method = class.get_method_by_args(method_name, args);
        env.call_method(instance, &method.name, &method.signature, args)
            .log_expect(
                format!(
                    "Error when calling method {} in class {} with method signature {}",
                    method.name, class.name, method.signature
                )
                .as_str(),
            )
    }

    pub fn get_static_field(
        &self,
        class_type: MinecraftClassType,
        field_name: &str,
        field_type: FieldType,
    ) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let jclass = env
            .find_class(&class.name)
            .log_expect(format!("{} class not found", class_type.get_name()).as_str());
        let field = class.get_field(field_name);
        env.get_static_field(jclass, &field.name, field_type.get_signature())
            .log_expect(format!("Error when getting static field {}", field.name).as_str())
    }

    pub fn get_field(
        &self,
        class_type: MinecraftClassType,
        instance: &JObject,
        field_name: &str,
        field_type: FieldType,
    ) -> JValueOwned {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let field = class.get_field(field_name);

        env.get_field(instance, &field.name, field_type.get_signature())
            .log_expect(format!("Error when getting field {}", field.name).as_str())
    }

    pub fn set_field(
        &self,
        class_type: MinecraftClassType,
        instance: &JObject,
        field_name: &str,
        field_type: FieldType,
        value: JValue,
    ) {
        let mut env = self.get_env();

        let class = self.get_class(class_type.get_name());
        let field = class.get_field(field_name);
        env.set_field(instance, &field.name, field_type.get_signature(), value)
            .log_expect(format!("Error when setting field {}", field.name).as_str());
    }

    pub fn new_global_ref(&self, obj: JObject) -> GlobalRef {
        let env = self.get_env();
        env.new_global_ref(obj).unwrap()
    }

    pub fn get_string(&self, obj: JObject) -> String {
        let env = self.get_env();
        let jstring = JString::from(obj);
        unsafe {
            let value = env
                .get_string_unchecked(jstring.as_ref())
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            value
        }
    }
}

impl Default for Mapping {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Display for better error messages
impl std::fmt::Display for MinecraftClassType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_type_extraction() {
        let class = MinecraftClass {
            name: "TestClass".to_string(),
            methods: HashMap::new(),
            fields: HashMap::new(),
        };

        // Test basic types
        assert_eq!(
            class.extract_parameter_types("()V").unwrap(),
            Vec::<String>::new()
        );

        assert_eq!(
            class.extract_parameter_types("(I)V").unwrap(),
            vec!["I"]
        );

        assert_eq!(
            class.extract_parameter_types("(ILjava/lang/String;F)V").unwrap(),
            vec!["I", "Ljava/lang/String;", "F"]
        );

        // Test arrays
        assert_eq!(
            class.extract_parameter_types("([I)V").unwrap(),
            vec!["[I"]
        );
    }

    #[test]
    fn test_type_compatibility() {
        let class = MinecraftClass {
            name: "TestClass".to_string(),
            methods: HashMap::new(),
            fields: HashMap::new(),
        };

        // Test exact matches
        assert_eq!(
            class.check_type_compatibility("I", &JValue::Int(42)),
            SignatureMatch::Exact
        );

        // Test compatible matches (promotion)
        assert_eq!(
            class.check_type_compatibility("I", &JValue::Byte(42)),
            SignatureMatch::Compatible
        );

        // Test incompatible matches
        assert_eq!(
            class.check_type_compatibility("I", &JValue::Double(42.0)),
            SignatureMatch::Incompatible
        );
    }
}