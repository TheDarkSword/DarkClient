use crate::state;
use anyhow::anyhow;
use jni::objects::{JClass, JObject, JString, JValue, JValueOwned};
use jni::JNIEnv;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A Minecraft class with its methods, as discovered by reflection.
#[derive(Debug)]
pub struct MinecraftClass {
    pub name: String,
    methods: HashMap<String, Vec<Method>>,
}

/// A method with its name and JNI signature.
#[derive(Debug)]
pub struct Method {
    pub name: String,
    pub signature: String,
    /// JNI method id, resolved on first call and cached here. The id is stable
    /// for the life of the JVM, so this turns every later call into a direct
    /// invocation with no name/signature lookup.
    pub id: OnceLock<MethodHandle>,
}

/// A cached JNI `jmethodID`. The id is a stable handle, valid from any thread
/// for as long as its class stays loaded, so sharing it is sound.
#[derive(Debug, Clone, Copy)]
pub struct MethodHandle(pub jni::sys::jmethodID);

unsafe impl Send for MethodHandle {}
unsafe impl Sync for MethodHandle {}

/// Signature matching result for method resolution
#[derive(Debug, PartialEq)]
enum SignatureMatch {
    Exact,
    Compatible,
    Incompatible,
}

#[allow(dead_code)]
impl MinecraftClass {
    /// Builds a class entry from runtime reflection: names are the real ones,
    /// only signatures were discovered. Fields need no entry — they are
    /// accessed by name with a descriptor the caller knows.
    pub fn from_reflection(name: String, methods: HashMap<String, Vec<Method>>) -> Self {
        MinecraftClass { name, methods }
    }

    pub fn get_method(&self, name: &str) -> anyhow::Result<&Method> {
        self.methods
            .get(name)
            .and_then(|overloads| overloads.first())
            .ok_or_else(|| anyhow!("{} method not found", name))
    }

    pub fn get_methods(&self, name: &str) -> anyhow::Result<&Vec<Method>> {
        match self.methods.get(name) {
            Some(methods) => Ok(methods),
            None => Err(anyhow!("{} method not found", name)),
        }
    }

    pub fn get_method_by_signature(&self, name: &str, signature: &str) -> anyhow::Result<&Method> {
        let methods = self.get_methods(name)?;
        match methods.iter().find(|method| method.signature == signature) {
            Some(method) => Ok(method),
            None => Err(anyhow!(
                "{} method with signature {} not found",
                name,
                signature
            )),
        }
    }

    pub fn get_method_by_args(&self, name: &str, args: &[JValue]) -> anyhow::Result<&Method> {
        let methods = self.get_methods(name)?;

        // If only one method exists, return it immediately
        if methods.len() == 1 {
            return Ok(&methods[0]);
        }

        // Find the best matching method based on argument compatibility
        let mut best_method = None;
        let mut best_match_quality = SignatureMatch::Incompatible;

        for method in methods {
            let match_quality = self.evaluate_signature_compatibility(&method.signature, args);

            if match_quality == SignatureMatch::Exact {
                // Exact match found, return immediately
                return Ok(method);
            }

            if match_quality == SignatureMatch::Compatible
                && best_match_quality != SignatureMatch::Exact
            {
                best_method = Some(method);
                best_match_quality = match_quality;
            }
        }

        match best_method {
            Some(method) => {
                log::debug!(
                    "Using compatible method '{}' with signature '{}' for args",
                    name,
                    method.signature
                );
                Ok(method)
            }
            None => {
                log::warn!(
                    "No compatible method found for '{}' with {} arguments, using first available method",
                    name, args.len()
                );
                Ok(&methods[0])
            }
        }
    }

    /// Evaluates how well a method signature matches the provided arguments
    fn evaluate_signature_compatibility(
        &self,
        method_signature: &str,
        args: &[JValue],
    ) -> SignatureMatch {
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
        let start = signature
            .find('(')
            .ok_or("Invalid signature: missing opening parenthesis")?;
        let end = signature
            .find(')')
            .ok_or("Invalid signature: missing closing parenthesis")?;

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
                    for ch in chars.by_ref() {
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
                                for ch in chars.by_ref() {
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
            ("I", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_)) => {
                SignatureMatch::Compatible
            }
            ("J", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_) | JValue::Int(_)) => {
                SignatureMatch::Compatible
            }
            ("F", JValue::Byte(_) | JValue::Short(_) | JValue::Char(_) | JValue::Int(_)) => {
                SignatureMatch::Compatible
            }
            (
                "D",
                JValue::Byte(_)
                | JValue::Short(_)
                | JValue::Char(_)
                | JValue::Int(_)
                | JValue::Long(_)
                | JValue::Float(_),
            ) => SignatureMatch::Compatible,

            // Object types - with proper type checking
            (jni_type, JValue::Object(obj))
                if jni_type.starts_with('L') && jni_type.ends_with(';') =>
            unsafe { self.check_object_type_compatibility(jni_type, obj) },

            // Arrays
            (jni_type, JValue::Object(obj)) if jni_type.starts_with('[') => unsafe {
                self.check_array_type_compatibility(jni_type, obj)
            },

            // Null handling - null can be assigned to any object type
            (jni_type, JValue::Object(obj))
                if jni_type.starts_with('L') || jni_type.starts_with('[') =>
            {
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
    unsafe fn check_object_type_compatibility(
        &self,
        expected_type: &str,
        obj: &JObject,
    ) -> SignatureMatch {
        // Handle null objects - they're compatible with any object type
        if obj.is_null() {
            return SignatureMatch::Compatible;
        }

        // Get the actual class name from the JNI type signature
        // Convert "Ljava/lang/String;" to "java/lang/String"
        let expected_class_name = &expected_type[1..expected_type.len() - 1];

        // Special case for java.lang.Object - everything is compatible
        if expected_class_name == "java/lang/Object" {
            return SignatureMatch::Compatible;
        }

        // Get JNI environment to check actual object type
        if let Ok(mut env) = state::env() {
            // Get the actual class of the object
            if let Ok(obj_class) = env.get_object_class(obj) {
                // Resolve through the captured game class loader: `find_class`
                // cannot see `net.minecraft.*` from a native (render) thread,
                // and a failed `find_class` leaks a pending JNI exception that
                // poisons the next JNI call (e.g. the following `GetMethodID`).
                if let Ok(expected_class) =
                    state::mapping().resolve_class(&mut env, expected_class_name)
                {
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
    unsafe fn check_array_type_compatibility(
        &self,
        expected_type: &str,
        obj: &JObject,
    ) -> SignatureMatch {
        // Handle null arrays
        if obj.is_null() {
            return SignatureMatch::Compatible;
        }

        if let Ok(mut env) = state::env() {
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
    unsafe fn get_class_name_from_object(
        &self,
        env: &mut JNIEnv,
        class: &JClass,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Get the Class.getName() method to retrieve the class name
        let class_class = env.find_class("java/lang/Class")?;
        let get_name_method = env.get_method_id(&class_class, "getName", "()Ljava/lang/String;")?;

        // Call getName() on the class object
        let name_obj = env.call_method_unchecked(
            class,
            get_name_method,
            jni::signature::ReturnType::Object,
            &[],
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
        if let (Some(actual_component), Some(expected_component)) = (
            self.extract_array_component_type(actual_type),
            self.extract_array_component_type(expected_type),
        ) {
            // For primitive arrays, they must match exactly
            if actual_component.len() == 1 && expected_component.len() == 1 {
                return actual_component == expected_component;
            }

            // For object arrays, check object compatibility
            if actual_component.starts_with('L') && expected_component.starts_with('L') {
                let actual_class = &actual_component[1..actual_component.len() - 1];
                let expected_class = &expected_component[1..expected_component.len() - 1];
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

    /// Names of every mapped method — for diagnostics only.
    pub fn method_names(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
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
        };

        // Test basic types
        assert_eq!(
            class.extract_parameter_types("()V").unwrap(),
            Vec::<String>::new()
        );

        assert_eq!(class.extract_parameter_types("(I)V").unwrap(), vec!["I"]);

        assert_eq!(
            class
                .extract_parameter_types("(ILjava/lang/String;F)V")
                .unwrap(),
            vec!["I", "Ljava/lang/String;", "F"]
        );

        // Test arrays
        assert_eq!(class.extract_parameter_types("([I)V").unwrap(), vec!["[I"]);
    }

    #[test]
    fn test_type_compatibility() {
        let class = MinecraftClass {
            name: "TestClass".to_string(),
            methods: HashMap::new(),
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

    #[test]
    fn overload_resolution_prefers_an_exact_primitive_match() {
        let mut methods = HashMap::new();
        methods.insert(
            "foo".to_string(),
            vec![
                Method {
                    name: "foo".into(),
                    signature: "(D)V".into(),
                    id: OnceLock::new(),
                },
                Method {
                    name: "foo".into(),
                    signature: "(I)V".into(),
                    id: OnceLock::new(),
                },
            ],
        );
        let class = MinecraftClass {
            name: "T".into(),
            methods,
        };

        let chosen = class.get_method_by_args("foo", &[JValue::Int(7)]).unwrap();
        assert_eq!(chosen.signature, "(I)V");
    }
}
