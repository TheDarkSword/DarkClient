use std::cell::Cell;
use std::sync::{OnceLock, RwLock};

use dashmap::DashMap;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{jsize, JNI_GetCreatedJavaVMs, JNI_OK};
use jni::{JNIEnv, JavaVM};
use log::{info, warn};
use mc_versions::MinecraftVersion;

use crate::{loader, Cx, Error, Result};

/// The injected session's view of the game: JVM, game class loader, class
/// cache and Minecraft version.
#[derive(Debug)]
pub struct Runtime {
    /// Handle to the host JVM; the source of every [`JNIEnv`].
    jvm: JavaVM,
    version: MinecraftVersion,
    /// The version id exactly as the game reports it (`"26.3"`), if it could
    /// be read.
    version_id: Option<String>,
    /// The class loader that runs the game. On modded builds (Fabric / Forge)
    /// it is discovered up-front by [`loader::discover_game_loader`]; on vanilla
    /// it is captured the first time a class resolves. `JNIEnv::find_class` is
    /// classloader-sensitive and only works from threads with a Minecraft Java
    /// frame on the stack — and on modded builds resolves a dead duplicate of
    /// the game classes — so routing every later lookup through this loader
    /// makes resolution both thread-independent and mod-loader-correct.
    class_loader: RwLock<Option<GlobalRef>>,
    /// Resolved JVM classes — and known-missing ones (`None`) — keyed by JNI
    /// name, so a class is searched for at most once.
    classes: DashMap<String, Option<GlobalRef>>,
    /// Keyed by the address of the constant's `Field` target.
    constants: DashMap<usize, GlobalRef>,
    /// Which binding table the generated bindings use for this version,
    /// computed on first use.
    binding_group: OnceLock<usize>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

thread_local! {
    /// A plain pointer on purpose: no TLS destructor, so unloading isn't pinned.
    static THREAD_ENV: Cell<*mut jni::sys::JNIEnv> = const { Cell::new(std::ptr::null_mut()) };
}

pub fn runtime() -> Result<&'static Runtime> {
    RUNTIME.get().ok_or(Error::NoRuntime)
}

/// Whether the vanilla `Minecraft` class resolves from this thread.
/// Obfuscated builds only have it under a scrambled name.
fn minecraft_class_resolves(env: &mut JNIEnv) -> bool {
    let found = env.find_class("net/minecraft/client/Minecraft").is_ok();
    if !found {
        let _ = env.exception_clear();
    }
    found
}

/// Obtains a handle to the JVM running in this process.
fn acquire_jvm() -> Result<JavaVM> {
    let mut raw: *mut jni::sys::JavaVM = std::ptr::null_mut();
    let mut count: jsize = 0;
    // SAFETY: standard JNI invocation-API call; both out-parameters are valid.
    unsafe {
        if JNI_GetCreatedJavaVMs(&mut raw, 1, &mut count) != JNI_OK || count == 0 {
            return Err(Error::NoJvm);
        }
        Ok(JavaVM::from_raw(raw)?)
    }
}

impl Runtime {
    /// Attaches to the game's JVM, finds the class loader that runs the game
    /// and reads the Minecraft version.
    pub fn new() -> Result<Runtime> {
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
            None => return Err(Error::NotMinecraft),
        }

        let mut runtime = Runtime {
            jvm,
            version: MinecraftVersion::LATEST,
            version_id: None,
            class_loader: RwLock::new(game_loader),
            classes: DashMap::new(),
            constants: DashMap::new(),
            binding_group: OnceLock::new(),
        };
        runtime.read_version();
        Ok(runtime)
    }

    pub fn install() -> Result<&'static Runtime> {
        if let Some(runtime) = RUNTIME.get() {
            return Ok(runtime);
        }
        let runtime = Runtime::new()?;
        Ok(RUNTIME.get_or_init(|| runtime))
    }

    /// Attaches the thread as a daemon the first time.
    pub fn env(&self) -> Result<JNIEnv<'_>> {
        let cached = THREAD_ENV.with(Cell::get);
        if !cached.is_null() {
            // SAFETY: obtained on this thread; a daemon attachment lasts as long as the thread.
            return Ok(unsafe { JNIEnv::from_raw(cached)? });
        }
        let env = self.jvm.attach_current_thread_as_daemon()?;
        THREAD_ENV.with(|slot| slot.set(env.get_raw()));
        Ok(env)
    }

    /// A binding context for the current thread.
    pub fn cx(&self) -> Result<Cx<'_>> {
        Ok(Cx::new(self, self.env()?))
    }

    /// The running version, parsed — [`MinecraftVersion::LATEST`] if unknown.
    pub fn version(&self) -> MinecraftVersion {
        self.version
    }

    /// The running version id exactly as the game reports it, if known.
    pub fn version_id(&self) -> Option<&str> {
        self.version_id.as_deref()
    }

    /// The class loader that runs the game, once known — used to
    /// `DefineClass` new classes that must see Minecraft types.
    pub fn game_class_loader(&self) -> Option<GlobalRef> {
        self.class_loader.read().ok().and_then(|slot| slot.clone())
    }

    /// The binding table index for this version, computed once by `select`.
    pub fn binding_group(&self, select: impl FnOnce(&Runtime) -> usize) -> usize {
        *self.binding_group.get_or_init(|| select(self))
    }

    /// Releases the JVM global references the runtime holds — every resolved
    /// class and the game class loader. Afterwards lookups re-resolve lazily.
    pub fn teardown(&self) {
        self.classes.clear();
        self.constants.clear();
        if let Ok(mut loader) = self.class_loader.write() {
            *loader = None;
        }
    }

    /// Reads `SharedConstants.getCurrentVersion().id()` through the game
    /// class loader — the same on vanilla, Fabric and Forge. An id that does
    /// not parse keeps [`MinecraftVersion::LATEST`].
    fn read_version(&mut self) {
        let read = || -> Result<String> {
            let mut env = self.env()?;
            env.with_local_frame(8, |env| -> Result<String> {
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
                    None => {
                        warn!("Unrecognized Minecraft version id {id:?}; assuming latest names")
                    }
                }
                self.version_id = Some(id);
            }
            Err(e) => {
                if let Ok(env) = self.env() {
                    let _ = env.exception_clear();
                }
                warn!("Could not read the Minecraft version ({e}); assuming latest names");
            }
        }
    }

    /// Resolves a JVM class by its JNI name, working from any thread.
    ///
    /// `JNIEnv::find_class` resolves against the class loader of the calling
    /// thread; on a thread with no Minecraft Java frame on its stack (the
    /// render thread, for instance) it cannot see `net.minecraft.*` classes.
    /// Once the game class loader is known every lookup goes through
    /// `ClassLoader.loadClass` on it instead.
    ///
    /// Results — hits *and* misses — are cached: a repeated lookup is one map
    /// read, and a missing class is never searched twice (a failed `loadClass`
    /// walks the whole classpath, ruinously slow to repeat per entity).
    pub fn resolve_class<'a>(&self, env: &mut JNIEnv<'a>, jni_name: &str) -> Result<JClass<'a>> {
        if let Some(cached) = self.classes.get(jni_name) {
            return match cached.value() {
                Some(handle) => Ok(JClass::from(env.new_local_ref(handle.as_obj())?)),
                None => Err(Error::ClassNotFound(jni_name.to_owned())),
            };
        }

        let resolved = self.lookup_class(env, jni_name);
        let handle = match &resolved {
            Ok(jclass) => env.new_global_ref(jclass).ok(),
            Err(_) => None,
        };
        if handle.is_none() {
            warn!("class '{jni_name}' could not be resolved at runtime");
        }
        self.classes.insert(jni_name.to_owned(), handle);
        resolved
    }

    pub fn class_global(&self, env: &mut JNIEnv, jni_name: &str) -> Result<GlobalRef> {
        if let Some(cached) = self.classes.get(jni_name) {
            return match cached.value() {
                Some(handle) => Ok(handle.clone()),
                None => Err(Error::ClassNotFound(jni_name.to_owned())),
            };
        }
        let local = self.resolve_class(env, jni_name)?;
        let _ = env.delete_local_ref(local);
        match self.classes.get(jni_name).as_deref() {
            Some(Some(handle)) => Ok(handle.clone()),
            _ => Err(Error::ClassNotFound(jni_name.to_owned())),
        }
    }

    pub(crate) fn constant(&self, key: usize) -> Option<GlobalRef> {
        self.constants.get(&key).map(|entry| entry.value().clone())
    }

    pub(crate) fn store_constant(&self, key: usize, value: GlobalRef) {
        self.constants.insert(key, value);
    }

    /// Looks a class up from scratch: through the game class loader if known,
    /// otherwise through `find_class`.
    fn lookup_class<'a>(&self, env: &mut JNIEnv<'a>, jni_name: &str) -> Result<JClass<'a>> {
        if let Some(loader) = self.game_class_loader() {
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
                    Err(Error::ClassNotFound(jni_name.to_owned()))
                }
            };
        }

        // No loader known yet: fall back to `find_class` and remember the
        // loader that resolved it for every later lookup.
        match env.find_class(jni_name) {
            Ok(jclass) => {
                self.capture_class_loader(env, &jclass);
                Ok(jclass)
            }
            Err(_) => {
                let _ = env.exception_clear();
                Err(Error::ClassNotFound(jni_name.to_owned()))
            }
        }
    }

    /// Records the class loader of `jclass` as the game class loader.
    fn capture_class_loader(&self, env: &mut JNIEnv, jclass: &JClass) {
        if self.game_class_loader().is_some() {
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
}
