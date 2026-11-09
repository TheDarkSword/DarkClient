use crate::mapping::class::{Method, MethodHandle, MinecraftClass};
pub use crate::mapping::class_type::MinecraftClassType;
pub use crate::mapping::object::MappedObject;
use dashmap::DashMap;
use jni::objects::{GlobalRef, JClass, JMethodID, JObject, JString, JValue, JValueOwned};
use jni::signature::{Primitive, ReturnType};
use jni::sys::{jsize, jvalue, JNI_GetCreatedJavaVMs, JNI_OK};
use jni::{JNIEnv, JavaVM};
use log::info;
pub use mapping_derive::MappedObject;
pub use mc_versions::MinecraftVersion;
use std::sync::{Arc, RwLock};

pub mod block;
pub mod block_entity;
pub mod class;
pub mod class_type;
pub mod client;
pub mod component;
pub mod entity;
pub mod inventory;
pub mod java;
mod loader;
pub mod math;
pub mod object;
mod reflect;
mod renames;

#[cfg(test)]
mod jvm_test;

/// Bridges the Mojmap names the code uses to the running JVM. Minecraft 26.1+
/// ships unobfuscated, so class, method and field names are already the real
/// ones; JNI still needs method signatures, which are discovered once per
/// class by reflection ([`reflect`]) and cached.
#[derive(Debug)]
pub struct Mapping {
    /// Handle to the host JVM; the source of every [`JNIEnv`] this bridge uses.
    jvm: JavaVM,
    version: MinecraftVersion,
    /// The version id exactly as the game reports it (`"26.3"`), if it could
    /// be read — what the supported-version check compares.
    version_id: Option<String>,
    /// Classes reflected so far. A `DashMap` so the render thread reads it
    /// without contending on one global lock.
    classes: DashMap<String, Arc<MinecraftClass>>,
    /// The class loader that runs the game. On modded builds (Fabric / Forge)
    /// it is discovered up-front by [`loader::discover_game_loader`]; on vanilla
    /// it is captured the first time a class resolves. `JNIEnv::find_class` is
    /// classloader-sensitive and only works from threads with a Minecraft Java
    /// frame on the stack — and on modded builds resolves a dead duplicate of
    /// the game classes — so routing every later lookup through this loader
    /// makes resolution both thread-independent and mod-loader-correct.
    class_loader: RwLock<Option<GlobalRef>>,
    /// Cache of resolved JVM classes — and known-missing ones (`None`) — keyed
    /// by JNI name, so a class is searched for at most once.
    class_handles: DashMap<String, Option<GlobalRef>>,
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

/// Whether the vanilla (unobfuscated) `Minecraft` class resolves from this
/// thread. Obfuscated builds — no longer supported — only have it under a
/// scrambled name.
fn minecraft_class_resolves(env: &mut JNIEnv) -> bool {
    let found = env.find_class("net/minecraft/client/Minecraft").is_ok();
    if !found {
        let _ = env.exception_clear();
    }
    found
}

/// Obtains a handle to the JVM running in this process.
fn acquire_jvm() -> anyhow::Result<JavaVM> {
    let mut raw: *mut jni::sys::JavaVM = std::ptr::null_mut();
    let mut count: jsize = 0;

    // SAFETY: standard JNI invocation-API call; both out-parameters are valid.
    unsafe {
        if JNI_GetCreatedJavaVMs(&mut raw, 1, &mut count) != JNI_OK || count == 0 {
            return Err(anyhow::anyhow!("no JVM found in this process"));
        }
        Ok(JavaVM::from_raw(raw)?)
    }
}

#[allow(dead_code)]
impl Mapping {
    pub fn new() -> anyhow::Result<Mapping> {
        let jvm = acquire_jvm()?;
        let mut env = jvm.attach_current_thread_as_daemon()?;

        // Discover the loader that runs the game before anything else: on
        // Fabric/Forge the game lives in an isolated class loader and a plain
        // `find_class` resolves a dead duplicate of `Minecraft` whose static
        // `instance` is null — the "Minecraft is null" failure (see `loader`).
        let game_loader = loader::discover_game_loader(&mut env);
        match &game_loader {
            Some(_) => info!("Game class loader found — routing class resolution through it"),
            None if minecraft_class_resolves(&mut env) => {
                info!("Vanilla Minecraft detected — capturing its class loader on first use")
            }
            None => anyhow::bail!(
                "Minecraft classes not found: obfuscated builds (1.21.11 and older) are not supported"
            ),
        }

        let mut mapping = Mapping {
            jvm,
            version: MinecraftVersion::LATEST,
            version_id: None,
            classes: DashMap::new(),
            class_loader: RwLock::new(game_loader),
            class_handles: DashMap::new(),
        };
        mapping.read_version();
        Ok(mapping)
    }

    /// Attaches the current thread to the JVM and returns a JNI environment.
    /// Called on the `'static` global mapping, the environment is `'static`.
    pub fn get_env(&self) -> anyhow::Result<JNIEnv<'_>> {
        Ok(self.jvm.attach_current_thread_as_daemon()?)
    }

    /// Runs `f` inside a fresh JNI local-reference frame: every local reference
    /// `f` creates is released when it returns, while the value it yields (a
    /// plain value or a `GlobalRef`-backed wrapper) survives. This lets each
    /// wrapper method bound its own JNI garbage, so no caller manages frames.
    pub fn in_frame<T>(&self, f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        let mut env = self.get_env()?;
        env.with_local_frame(16, |_| f())
    }

    /// Releases the JVM global references this mapping holds — every resolved
    /// class handle and the captured game class loader. Called from
    /// `cleanup_client` before the library is unloaded; afterwards lookups
    /// would simply re-resolve lazily.
    pub fn teardown(&self) {
        self.classes.clear();
        self.class_handles.clear();
        if let Ok(mut loader) = self.class_loader.write() {
            *loader = None;
        }
    }

    pub fn get_version(&self) -> MinecraftVersion {
        self.version
    }

    /// The version id exactly as the game reports it, if it could be read.
    pub fn version_id(&self) -> Option<&str> {
        self.version_id.as_deref()
    }

    /// Reads the running version from `SharedConstants.getCurrentVersion().id()`
    /// — through the game class loader, so the same on vanilla, Fabric and
    /// Forge. An id that does not parse keeps [`MinecraftVersion::LATEST`],
    /// which resolves every name to its newest form.
    fn read_version(&mut self) {
        let read = || -> anyhow::Result<String> {
            let mut env = self.get_env()?;
            env.with_local_frame(8, |env| -> anyhow::Result<String> {
                let class = self.resolve_class(env, "net/minecraft/SharedConstants")?;
                let version = env
                    .call_static_method(
                        &class,
                        "getCurrentVersion",
                        "()Lnet/minecraft/WorldVersion;",
                        &[],
                    )?
                    .l()?;
                let id = env
                    .call_method(&version, "id", "()Ljava/lang/String;", &[])?
                    .l()?;
                Ok(env.get_string(&JString::from(id))?.into())
            })
        };

        match read() {
            Ok(id) => {
                match MinecraftVersion::parse_id(&id) {
                    Some(version) => {
                        info!("Minecraft {id} (version {version})");
                        self.version = version;
                    }
                    None => log::warn!(
                        "Unrecognized Minecraft version id {id:?}; assuming latest names"
                    ),
                }
                self.version_id = Some(id);
            }
            Err(e) => {
                if let Ok(env) = self.get_env() {
                    let _ = env.exception_clear();
                }
                log::warn!("Could not read the Minecraft version ({e}); assuming latest names");
            }
        }
    }

    /// Resolves a class by name, reflecting it from the JVM and caching it on
    /// first request.
    pub fn get_class(&self, name: &str) -> anyhow::Result<Arc<MinecraftClass>> {
        let name = renames::resolve_class(name, self.version);
        if let Some(class) = self.classes.get(name) {
            return Ok(Arc::clone(class.value()));
        }
        let class = Arc::new(reflect::reflect_class(self, name)?);
        self.classes.insert(name.to_owned(), Arc::clone(&class));
        Ok(class)
    }

    /// The captured Minecraft class loader, if any — poison-safe.
    fn loader(&self) -> Option<GlobalRef> {
        self.class_loader.read().ok().and_then(|slot| slot.clone())
    }

    /// The captured game class loader — used to `DefineClass` new classes (the
    /// Netty bridge handler) so they can see Minecraft and Netty types.
    pub fn game_class_loader(&self) -> Option<GlobalRef> {
        self.loader()
    }

    /// Resolves a JVM class by its JNI name, working from any thread.
    ///
    /// `JNIEnv::find_class` resolves against the class loader of the calling
    /// thread; on a thread with no Minecraft Java frame on its stack (the
    /// render thread, for instance) it cannot see `net.minecraft.*` classes.
    /// Once the Minecraft class loader has been captured every lookup goes
    /// through `ClassLoader.loadClass` on it instead.
    ///
    /// Results — hits *and* misses — are cached: a repeated lookup is one map
    /// read, and a missing class is never searched twice (a failed `loadClass`
    /// walks the whole classpath, ruinously slow to repeat per entity).
    pub(crate) fn resolve_class<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        jni_name: &str,
    ) -> anyhow::Result<JClass<'a>> {
        if let Some(cached) = self.class_handles.get(jni_name) {
            return match cached.value() {
                Some(handle) => Ok(JClass::from(env.new_local_ref(handle.as_obj())?)),
                None => Err(anyhow::anyhow!("Class {} not present at runtime", jni_name)),
            };
        }

        let resolved = self.lookup_class(env, jni_name);
        let handle = match &resolved {
            Ok(jclass) => env.new_global_ref(jclass).ok(),
            Err(_) => None,
        };
        if handle.is_none() {
            log::warn!(
                "Mapping: class '{}' could not be resolved at runtime",
                jni_name
            );
        }
        self.class_handles.insert(jni_name.to_owned(), handle);
        resolved
    }

    /// Looks a class up from scratch: through the captured Minecraft class
    /// loader if available, otherwise through `find_class`.
    fn lookup_class<'a>(&self, env: &mut JNIEnv<'a>, jni_name: &str) -> anyhow::Result<JClass<'a>> {
        if let Some(loader) = self.loader() {
            let binary_name = jni_name.replace('/', ".");
            let name: JObject = env.new_string(binary_name)?.into();
            return match env.call_method(
                loader.as_obj(),
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            ) {
                Ok(value) => Ok(JClass::from(value.l()?)),
                Err(_) => {
                    let _ = env.exception_clear();
                    Err(anyhow::anyhow!("Class {} not found at runtime", jni_name))
                }
            };
        }

        // No loader captured yet: fall back to `find_class` and remember the
        // loader that resolved it for every later lookup.
        match env.find_class(jni_name) {
            Ok(jclass) => {
                self.capture_class_loader(env, &jclass);
                Ok(jclass)
            }
            Err(_) => {
                let _ = env.exception_clear();
                Err(anyhow::anyhow!("Class {} not found at runtime", jni_name))
            }
        }
    }

    /// Records the class loader of `jclass` as the Minecraft class loader.
    fn capture_class_loader(&self, env: &mut JNIEnv, jclass: &JClass) {
        if self.loader().is_some() {
            return;
        }
        let loader = env.call_method(jclass, "getClassLoader", "()Ljava/lang/ClassLoader;", &[]);
        match loader.and_then(|value| value.l()) {
            Ok(obj) if !obj.is_null() => {
                if let (Ok(global), Ok(mut slot)) =
                    (env.new_global_ref(obj), self.class_loader.write())
                {
                    *slot = Some(global);
                }
            }
            _ => {
                let _ = env.exception_clear();
            }
        }
    }

    /// Runtime (JVM) name of a class.
    fn runtime_class_name(&self, class_type: MinecraftClassType) -> String {
        renames::resolve_class(class_type.get_name(), self.version).to_owned()
    }

    /// Runtime (JVM) name of a field.
    fn runtime_field_name(&self, class_type: MinecraftClassType, field: &str) -> String {
        renames::resolve_member(class_type, field, self.version).to_owned()
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
        let method_name = renames::resolve_member(class_type, method_name, self.version);
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
        let method_name = renames::resolve_member(class_type, method_name, self.version);
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
