use crate::mapping::class::{Method, MethodHandle, MinecraftClass};
pub use crate::mapping::class_type::MinecraftClassType;
pub use crate::mapping::object::MappedObject;
use dashmap::DashMap;
use jni::objects::{GlobalRef, JClass, JMethodID, JObject, JString, JValue, JValueOwned};
use jni::signature::{Primitive, ReturnType};
use jni::sys::jvalue;
use jni::JNIEnv;
pub use mapping_derive::MappedObject;
pub use mc_runtime::{Cx, Runtime};
pub use mc_versions::MinecraftVersion;
use std::sync::Arc;

pub mod block;
pub mod block_entity;
pub mod class;
pub mod class_type;
pub mod client;
pub mod component;
pub mod entity;
pub mod inventory;
pub mod java;
pub mod math;
pub mod object;
mod reflect;
mod renames;

#[cfg(test)]
mod jvm_test;

/// Bridges the Mojmap names the code uses to the running JVM. Minecraft 26.x
/// is unobfuscated, so class, method and field names are already the real
/// ones; JNI still needs method signatures, which are discovered once per
/// class by reflection ([`reflect`]) and cached. JVM, game class loader,
/// class cache and version live in the shared [`Runtime`], also used by the
/// generated bindings.
#[derive(Debug)]
pub struct Mapping {
    runtime: Runtime,
    /// Classes reflected so far. A `DashMap` so the render thread reads it
    /// without contending on one global lock.
    classes: DashMap<String, Arc<MinecraftClass>>,
}

#[allow(dead_code)]
pub enum FieldType {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    String,
    Object(MinecraftClassType),
}

impl FieldType {
    pub fn get_signature(&self) -> anyhow::Result<String> {
        Ok(match self {
            FieldType::Boolean => String::from("Z"),
            FieldType::Byte => String::from("B"),
            FieldType::Char => String::from("C"),
            FieldType::Short => String::from("S"),
            FieldType::Int => String::from("I"),
            FieldType::Long => String::from("J"),
            FieldType::Float => String::from("F"),
            FieldType::Double => String::from("D"),
            FieldType::String => String::from("Ljava/lang/String;"),
            FieldType::Object(class_type) => format!("L{};", class_type.get_name()),
        })
    }
}

#[allow(dead_code)]
impl Mapping {
    pub fn new() -> anyhow::Result<Mapping> {
        Ok(Mapping {
            runtime: Runtime::new()?,
            classes: DashMap::new(),
        })
    }

    /// The shared JNI runtime — what the generated bindings run on.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// A binding context for the current thread.
    pub fn cx(&self) -> anyhow::Result<Cx<'_>> {
        Ok(self.runtime.cx()?)
    }

    /// Attaches the current thread to the JVM and returns a JNI environment.
    /// Called on the `'static` global mapping, the environment is `'static`.
    pub fn get_env(&self) -> anyhow::Result<JNIEnv<'_>> {
        Ok(self.runtime.env()?)
    }

    /// Runs `f` inside a fresh JNI local-reference frame: every local reference
    /// `f` creates is released when it returns, while the value it yields (a
    /// plain value or a `GlobalRef`-backed wrapper) survives. This lets each
    /// wrapper method bound its own JNI garbage, so no caller manages frames.
    pub fn in_frame<T>(&self, f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        let mut env = self.get_env()?;
        env.with_local_frame(16, |_| f())
    }

    /// Releases the JVM global references this mapping holds — reflected
    /// classes, resolved class handles and the game class loader. Called from
    /// `cleanup_client` before the library is unloaded.
    pub fn teardown(&self) {
        self.classes.clear();
        self.runtime.teardown();
    }

    pub fn get_version(&self) -> MinecraftVersion {
        self.runtime.version()
    }

    /// The version id exactly as the game reports it, if it could be read.
    pub fn version_id(&self) -> Option<&str> {
        self.runtime.version_id()
    }

    /// Resolves a class by name, reflecting it from the JVM and caching it on
    /// first request.
    pub fn get_class(&self, name: &str) -> anyhow::Result<Arc<MinecraftClass>> {
        let name = renames::resolve_class(name, self.runtime.version());
        if let Some(class) = self.classes.get(name) {
            return Ok(Arc::clone(class.value()));
        }
        let class = Arc::new(reflect::reflect_class(self, name)?);
        self.classes.insert(name.to_owned(), Arc::clone(&class));
        Ok(class)
    }

    /// The game class loader — used to `DefineClass` new classes (the Netty
    /// bridge handler) so they can see Minecraft and Netty types.
    pub fn game_class_loader(&self) -> Option<GlobalRef> {
        self.runtime.game_class_loader()
    }

    /// Resolves a JVM class by its JNI name, working from any thread — see
    /// [`Runtime::resolve_class`].
    pub(crate) fn resolve_class<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        jni_name: &str,
    ) -> anyhow::Result<JClass<'a>> {
        Ok(self.runtime.resolve_class(env, jni_name)?)
    }

    /// Runtime (JVM) name of a class.
    fn runtime_class_name(&self, class_type: MinecraftClassType) -> String {
        renames::resolve_class(class_type.get_name(), self.runtime.version()).to_owned()
    }

    /// Runtime (JVM) name of a field.
    fn runtime_field_name(&self, class_type: MinecraftClassType, field: &str) -> String {
        renames::resolve_member(class_type, field, self.runtime.version()).to_owned()
    }

    pub fn call_static_method(
        &'_ self,
        class_type: MinecraftClassType,
        method_name: &str,
        args: &[JValue],
    ) -> anyhow::Result<JValueOwned<'_>> {
        let mut env = self.get_env()?;

        let class = self.get_class(class_type.get_name())?;
        let jclass = self.resolve_class(&mut env, &class.name)?;
        let method_name = renames::resolve_member(class_type, method_name, self.runtime.version());
        let method = class.get_method_by_args(method_name, args)?;
        match env.call_static_method(jclass, &method.name, &method.signature, args) {
            Ok(value) => Ok(value),
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!(
                    "Error calling static method {}{} in class {}",
                    method.name,
                    method.signature,
                    class.name,
                ))
            }
        }
    }

    pub fn call_method(
        &'_ self,
        class_type: MinecraftClassType,
        instance: &JObject,
        method_name: &str,
        args: &[JValue],
    ) -> anyhow::Result<JValueOwned<'_>> {
        let mut env = self.get_env()?;

        let class = self.get_class(class_type.get_name())?;
        let method_name = renames::resolve_member(class_type, method_name, self.runtime.version());
        let method = class.get_method_by_args(method_name, args)?;

        // `call_method_unchecked` does not validate arity — so guard it here,
        // since `get_method_by_args` skips that check for single-overload
        // methods.
        if signature_arg_count(&method.signature) != args.len() {
            return Err(anyhow::anyhow!(
                "Argument count mismatch calling {} ({}) — signature {}",
                method_name,
                method.name,
                method.signature
            ));
        }

        let method_id = self.resolve_method_id(&mut env, class_type, method)?;
        let return_type = parse_return_type(&method.signature);
        let jni_args: Vec<jvalue> = args.iter().map(|arg| arg.as_jni()).collect();

        match unsafe { env.call_method_unchecked(instance, method_id, return_type, &jni_args) } {
            Ok(value) => Ok(value),
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!(
                    "Error calling method {}{} in class {}",
                    method.name,
                    method.signature,
                    class.name,
                ))
            }
        }
    }

    /// Resolves the JNI method id for `method`, caching it on the `Method` so
    /// the costly name+signature lookup happens only once per method.
    fn resolve_method_id(
        &self,
        env: &mut JNIEnv,
        class_type: MinecraftClassType,
        method: &Method,
    ) -> anyhow::Result<JMethodID> {
        if let Some(handle) = method.id.get() {
            return Ok(unsafe { JMethodID::from_raw(handle.0) });
        }

        let class_name = self.runtime_class_name(class_type);
        let jclass = self.resolve_class(env, &class_name)?;
        let id = match env.get_method_id(&jclass, &method.name, &method.signature) {
            Ok(id) => id,
            Err(_) => {
                let _ = env.exception_clear();
                return Err(anyhow::anyhow!(
                    "Method {} {} not found on class {}",
                    method.name,
                    method.signature,
                    class_name
                ));
            }
        };
        let _ = method.id.set(MethodHandle(id.into_raw()));
        Ok(id)
    }

    pub fn get_static_field(
        &'_ self,
        class_type: MinecraftClassType,
        field_name: &str,
        field_type: FieldType,
    ) -> anyhow::Result<JValueOwned<'_>> {
        let mut env = self.get_env()?;

        let class_name = self.runtime_class_name(class_type);
        let jclass = self.resolve_class(&mut env, &class_name)?;
        let runtime_field = self.runtime_field_name(class_type, field_name);
        match env.get_static_field(jclass, &runtime_field, field_type.get_signature()?) {
            Ok(value) => Ok(value),
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!(
                    "Error getting static field {} ({}) from class {}",
                    field_name,
                    runtime_field,
                    class_type.get_name()
                ))
            }
        }
    }

    pub fn get_field(
        &'_ self,
        class_type: MinecraftClassType,
        instance: &JObject,
        field_name: &str,
        field_type: FieldType,
    ) -> anyhow::Result<JValueOwned<'_>> {
        let mut env = self.get_env()?;

        let runtime_field = self.runtime_field_name(class_type, field_name);
        match env.get_field(instance, &runtime_field, field_type.get_signature()?) {
            Ok(value) => Ok(value),
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!(
                    "Error getting field {} ({}) from class {}",
                    field_name,
                    runtime_field,
                    class_type.get_name()
                ))
            }
        }
    }

    pub fn set_field(
        &self,
        class_type: MinecraftClassType,
        instance: &JObject,
        field_name: &str,
        field_type: FieldType,
        value: JValue,
    ) -> anyhow::Result<()> {
        let mut env = self.get_env()?;

        let runtime_field = self.runtime_field_name(class_type, field_name);
        match env.set_field(instance, &runtime_field, field_type.get_signature()?, value) {
            Ok(_) => Ok(()),
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!(
                    "Error setting field {} ({}) in class {}",
                    field_name,
                    runtime_field,
                    class_type.get_name()
                ))
            }
        }
    }

    pub fn new_global_ref(&self, obj: JObject) -> anyhow::Result<GlobalRef> {
        let env = self.get_env()?;
        Ok(env.new_global_ref(obj)?)
    }

    pub fn get_string(&self, obj: JObject) -> anyhow::Result<String> {
        let env = self.get_env()?;
        let jstring = JString::from(obj);
        unsafe {
            let value = env
                .get_string_unchecked(jstring.as_ref())?
                .to_str()?
                .to_string();
            Ok(value)
        }
    }

    pub fn is_instance_of(
        &self,
        class_type: MinecraftClassType,
        instance: &JObject,
    ) -> anyhow::Result<bool> {
        let mut env = self.get_env()?;
        let class_name = self.runtime_class_name(class_type);
        let jclass = self.resolve_class(&mut env, &class_name)?;
        Ok(env.is_instance_of(instance, &jclass)?)
    }
}

/// Number of parameters in a JNI method signature, e.g. `(ILjava/lang/String;)V`
/// has 2. Used to guard the unchecked call path against arity mismatches.
fn signature_arg_count(signature: &str) -> usize {
    let params = match (signature.find('('), signature.find(')')) {
        (Some(open), Some(close)) if open < close => &signature[open + 1..close],
        // Unparseable: return a count that can never match a real call.
        _ => return usize::MAX,
    };

    let mut count = 0;
    let mut chars = params.chars();
    while let Some(ch) = chars.next() {
        match ch {
            // Array prefix — the descriptor it belongs to is counted next.
            '[' => continue,
            // Object descriptor runs until its terminating ';'.
            'L' => {
                for inner in chars.by_ref() {
                    if inner == ';' {
                        break;
                    }
                }
                count += 1;
            }
            // Any primitive.
            _ => count += 1,
        }
    }
    count
}

/// Maps the return descriptor of a JNI signature to a [`ReturnType`].
fn parse_return_type(signature: &str) -> ReturnType {
    let return_descriptor = signature.rsplit(')').next().unwrap_or("V");
    match return_descriptor.chars().next() {
        Some('Z') => ReturnType::Primitive(Primitive::Boolean),
        Some('B') => ReturnType::Primitive(Primitive::Byte),
        Some('C') => ReturnType::Primitive(Primitive::Char),
        Some('S') => ReturnType::Primitive(Primitive::Short),
        Some('I') => ReturnType::Primitive(Primitive::Int),
        Some('J') => ReturnType::Primitive(Primitive::Long),
        Some('F') => ReturnType::Primitive(Primitive::Float),
        Some('D') => ReturnType::Primitive(Primitive::Double),
        Some('[') => ReturnType::Array,
        Some('L') => ReturnType::Object,
        _ => ReturnType::Primitive(Primitive::Void),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_count_counts_primitives_and_objects() {
        assert_eq!(signature_arg_count("()V"), 0);
        assert_eq!(signature_arg_count("(I)V"), 1);
        assert_eq!(signature_arg_count("(ILjava/lang/String;F)V"), 3);
        assert_eq!(signature_arg_count("([ILjava/lang/String;)V"), 2);
    }

    #[test]
    fn arg_count_of_an_unparseable_signature_never_matches() {
        assert_eq!(signature_arg_count("garbage"), usize::MAX);
    }

    #[test]
    fn return_type_is_parsed_from_the_descriptor() {
        assert!(matches!(
            parse_return_type("()V"),
            ReturnType::Primitive(Primitive::Void)
        ));
        assert!(matches!(
            parse_return_type("(I)I"),
            ReturnType::Primitive(Primitive::Int)
        ));
        assert!(matches!(
            parse_return_type("()Z"),
            ReturnType::Primitive(Primitive::Boolean)
        ));
        assert!(matches!(
            parse_return_type("()Ljava/lang/String;"),
            ReturnType::Object
        ));
        assert!(matches!(parse_return_type("()[I"), ReturnType::Array));
    }

    #[test]
    fn primitive_field_types_produce_jni_signatures() {
        assert_eq!(FieldType::Boolean.get_signature().unwrap(), "Z");
        assert_eq!(FieldType::Int.get_signature().unwrap(), "I");
        assert_eq!(FieldType::Long.get_signature().unwrap(), "J");
        assert_eq!(FieldType::Double.get_signature().unwrap(), "D");
        assert_eq!(
            FieldType::String.get_signature().unwrap(),
            "Ljava/lang/String;"
        );
    }
}
