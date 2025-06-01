use crate::mapping::{GameContext, Mapping, MinecraftClassType};
use jni::objects::GlobalRef;
use jni::sys::jlong;

#[derive(Debug)]
pub struct Window {
    pub jni_ref: GlobalRef,
}

impl GameContext for Window {}

impl Window {
    pub fn new(minecraft: &GlobalRef, mapping: &Mapping) -> Window {
        let window_obj = mapping
            .call_method(
                MinecraftClassType::Minecraft,
                minecraft.as_obj(),
                "getWindow",
                &[],
            )
            .l()
            .unwrap();

        Window {
            jni_ref: mapping.new_global_ref(window_obj),
        }
    }

    pub fn get_window(&self) -> jlong {
        let mapping = self.mapping();

        mapping
            .call_method(
                MinecraftClassType::Window,
                self.jni_ref.as_obj(),
                "getWindow",
                &[],
            )
            .j()
            .unwrap()
    }
}
