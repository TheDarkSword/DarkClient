use crate::module::{Module, ModuleType};
use crate::LogExpect;
use jni::sys::{jsize, JNI_GetCreatedJavaVMs, JNI_OK};
use jni::{JNIEnv, JavaVM};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

#[derive(Debug)]
pub struct DarkClient {
    pub(crate) jvm: Arc<JavaVM>,
    modules: Arc<RwLock<HashMap<String, Arc<Mutex<ModuleType>>>>>,
}

impl DarkClient {
    pub fn instance() -> &'static DarkClient {
        static INSTANCE: OnceLock<Arc<DarkClient>> = OnceLock::new();

        INSTANCE.get_or_init(|| unsafe {
            Arc::new(DarkClient::new().log_expect("Failed to create DarkClient"))
        })
    }

    pub unsafe fn new() -> Result<Self, &'static str> {
        let mut java_vm: *mut jni::sys::JavaVM = std::ptr::null_mut();
        let mut count: jsize = 0;

        if JNI_GetCreatedJavaVMs(&mut java_vm, 1, &mut count) != JNI_OK || count == 0 {
            return Err("Failed to get Java VMs");
        }

        let java_vm: Arc<JavaVM> = Arc::new(match JavaVM::from_raw(java_vm) {
            Ok(jvm) => jvm,
            Err(_) => return Err("Could not get JavaVM"),
        });

        Ok(DarkClient {
            jvm: java_vm,
            modules: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn get_env(&self) -> jni::errors::Result<JNIEnv> {
        //self.jvm.attach_current_thread()
        self.jvm.attach_current_thread_as_daemon()
    }

    pub fn register_module(&self, module: Arc<Mutex<dyn Module + Send + Sync>>) {
        let module_name = module.lock().unwrap().get_module_data().name.clone();
        self.modules.write().unwrap().insert(module_name, module);
    }

    pub fn tick(&self) {
        let modules = self.modules.read().unwrap();
        for module in modules.values() {
            let module = module.lock().unwrap();
            if module.get_module_data().enabled {
                module.on_tick();
            }
        }
    }
}

// Module for handling keyboard inputs
pub mod keyboard {
    use super::*;
    use crate::mapping::client::minecraft::Minecraft;
    use jni::objects::JValue;
    use jni::sys::jlong;
    use log::info;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicBool;
    use std::thread;
    use std::time::Duration;

    static RUNNING: OnceLock<AtomicBool> = OnceLock::new();

    pub fn start_keyboard_handler() {
        if RUNNING.get().is_none() {
            RUNNING.set(AtomicBool::new(true)).unwrap();
        }
        thread::spawn(|| {
            let minecraft = Minecraft::instance();
            let client = DarkClient::instance();
            let mut env = client.get_env().unwrap();

            let glfw_window = minecraft.window.get_window();

            let mut keys: HashSet<i32> = HashSet::new();
            while RUNNING
                .get()
                .unwrap()
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                thread::sleep(Duration::from_millis(100));

                client.modules.read().unwrap().values().for_each(|module| {
                    let mut module = module.lock().unwrap();
                    let module_data = module.get_module_data();
                    let key = module_data.key_bind as i32;

                    if is_key_down(&mut env, glfw_window, key) {
                        if !keys.contains(&key) {
                            keys.insert(key);

                            let enabled = !module_data.enabled;
                            info!(
                                "{} {}",
                                module_data.name,
                                if enabled { "enabled" } else { "disabled" }
                            );
                            if enabled {
                                module.on_start();
                            } else {
                                module.on_stop();
                            }
                            module.get_module_data_mut().set_enabled(enabled);
                        }
                    } else {
                        keys.remove(&key);
                    }
                });
            }
        });
    }

    pub fn stop_keyboard_handler() {
        if RUNNING.get().is_none() {
            return;
        }
        RUNNING
            .get()
            .unwrap()
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn is_key_down(env: &mut JNIEnv, glfw_window: jlong, key: i32) -> bool {
        let glfw = env.find_class("org/lwjgl/glfw/GLFW").unwrap();
        env.call_static_method(
            glfw,
            "glfwGetKey",
            "(JI)I",
            &[JValue::Long(glfw_window), JValue::Int(key)],
        )
        .unwrap()
        .i()
        .unwrap()
            == 1
    }
}
