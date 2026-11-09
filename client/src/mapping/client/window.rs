use crate::mapping::{MappedObject, MinecraftClassType};
use crate::state::mapping;
use jni::objects::GlobalRef;
use jni::sys::jlong;

#[derive(Debug, MappedObject)]
#[mapped(class = Window)]
pub struct Window {
    jni_ref: GlobalRef,
}

impl Window {
    pub fn new(minecraft: &GlobalRef) -> anyhow::Result<Window> {
        mapping().in_frame(|| {
            let window_obj = mapping()
                .call_method(
                    MinecraftClassType::Minecraft,
                    minecraft.as_obj(),
                    "getWindow",
                    &[],
                )?
                .l()?;
            Ok(Window {
                jni_ref: mapping().new_global_ref(window_obj)?,
            })
        })
    }

    /// The native window handle: a `GLFWwindow*` up to 26.2, an
    /// `SDL_Window*` from 26.3.
    pub fn get_window(&self) -> anyhow::Result<jlong> {
        Ok(self.call_method("handle", &[])?.j()?)
    }
}
