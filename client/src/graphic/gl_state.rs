//! Save / restore of the OpenGL state the overlay touches.
//!
//! Minecraft's `GlStateManager` caches GL state on the Java side and skips
//! calls it believes redundant. Any state the overlay leaves changed is state
//! Minecraft never re-applies: the next frame draws the sky with egui's blend
//! function, terrain with culling off, and so on. [`GlStateGuard`] captures
//! everything the egui painter and our own setup change, and puts it back
//! when dropped.

use crate::gl;
use crate::gl::types::{GLboolean, GLenum, GLint};

pub struct GlStateGuard {
    blend: bool,
    blend_func: [GLint; 4],
    blend_equation: [GLint; 2],
    cull_face: bool,
    depth_test: bool,
    scissor_test: bool,
    scissor_box: [GLint; 4],
    color_mask: [GLboolean; 4],
    viewport: [GLint; 4],
    framebuffer_srgb: bool,
    program: GLint,
    vertex_array: GLint,
    array_buffer: GLint,
    element_array_buffer: GLint,
    pixel_unpack_buffer: GLint,
    active_texture: GLint,
    texture_2d_unit0: GLint,
    sampler_unit0: GLint,
    unpack: [(GLenum, GLint); 4],
}

const UNPACK_PARAMS: [GLenum; 4] = [
    gl::UNPACK_ALIGNMENT,
    gl::UNPACK_ROW_LENGTH,
    gl::UNPACK_SKIP_PIXELS,
    gl::UNPACK_SKIP_ROWS,
];

unsafe fn int(pname: GLenum) -> GLint {
    let mut value = 0;
    gl::GetIntegerv(pname, &mut value);
    value
}

unsafe fn ints<const N: usize>(pname: GLenum) -> [GLint; N] {
    let mut values = [0; N];
    gl::GetIntegerv(pname, values.as_mut_ptr());
    values
}

unsafe fn enabled(cap: GLenum) -> bool {
    gl::IsEnabled(cap) == gl::TRUE
}

unsafe fn set_enabled(cap: GLenum, on: bool) {
    if on {
        gl::Enable(cap);
    } else {
        gl::Disable(cap);
    }
}

impl GlStateGuard {
    /// Captures the current state.
    ///
    /// # Safety
    /// A GL context must be current on this thread, with function pointers
    /// loaded.
    pub unsafe fn capture() -> GlStateGuard {
        let active_texture = int(gl::ACTIVE_TEXTURE);
        // Texture and sampler bindings are per unit; egui only uses unit 0.
        gl::ActiveTexture(gl::TEXTURE0);
        let mut color_mask = [gl::FALSE; 4];
        gl::GetBooleanv(gl::COLOR_WRITEMASK, color_mask.as_mut_ptr());

        GlStateGuard {
            blend: enabled(gl::BLEND),
            blend_func: [
                int(gl::BLEND_SRC_RGB),
                int(gl::BLEND_DST_RGB),
                int(gl::BLEND_SRC_ALPHA),
                int(gl::BLEND_DST_ALPHA),
            ],
            blend_equation: [int(gl::BLEND_EQUATION_RGB), int(gl::BLEND_EQUATION_ALPHA)],
            cull_face: enabled(gl::CULL_FACE),
            depth_test: enabled(gl::DEPTH_TEST),
            scissor_test: enabled(gl::SCISSOR_TEST),
            scissor_box: ints(gl::SCISSOR_BOX),
            color_mask,
            viewport: ints(gl::VIEWPORT),
            framebuffer_srgb: enabled(gl::FRAMEBUFFER_SRGB),
            program: int(gl::CURRENT_PROGRAM),
            vertex_array: int(gl::VERTEX_ARRAY_BINDING),
            array_buffer: int(gl::ARRAY_BUFFER_BINDING),
            element_array_buffer: int(gl::ELEMENT_ARRAY_BUFFER_BINDING),
            pixel_unpack_buffer: int(gl::PIXEL_UNPACK_BUFFER_BINDING),
            active_texture,
            texture_2d_unit0: int(gl::TEXTURE_BINDING_2D),
            sampler_unit0: int(gl::SAMPLER_BINDING),
            unpack: UNPACK_PARAMS.map(|param| (param, int(param))),
        }
    }

    /// The viewport at capture time — the framebuffer size Minecraft renders at.
    pub fn viewport(&self) -> [GLint; 4] {
        self.viewport
    }
}

impl Drop for GlStateGuard {
    fn drop(&mut self) {
        // SAFETY: dropped on the thread that captured it, with the same
        // context still current.
        unsafe {
            set_enabled(gl::BLEND, self.blend);
            let [src_rgb, dst_rgb, src_alpha, dst_alpha] = self.blend_func.map(|f| f as GLenum);
            gl::BlendFuncSeparate(src_rgb, dst_rgb, src_alpha, dst_alpha);
            gl::BlendEquationSeparate(
                self.blend_equation[0] as GLenum,
                self.blend_equation[1] as GLenum,
            );
            set_enabled(gl::CULL_FACE, self.cull_face);
            set_enabled(gl::DEPTH_TEST, self.depth_test);
            set_enabled(gl::SCISSOR_TEST, self.scissor_test);
            let [x, y, w, h] = self.scissor_box;
            gl::Scissor(x, y, w, h);
            let [r, g, b, a] = self.color_mask;
            gl::ColorMask(r, g, b, a);
            let [x, y, w, h] = self.viewport;
            gl::Viewport(x, y, w, h);
            set_enabled(gl::FRAMEBUFFER_SRGB, self.framebuffer_srgb);

            gl::UseProgram(self.program as u32);
            // The element buffer is VAO state: bind the VAO first so the
            // element buffer lands on it, not on whatever egui left bound.
            gl::BindVertexArray(self.vertex_array as u32);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.element_array_buffer as u32);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.array_buffer as u32);
            gl::BindBuffer(gl::PIXEL_UNPACK_BUFFER, self.pixel_unpack_buffer as u32);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_2d_unit0 as u32);
            gl::BindSampler(0, self.sampler_unit0 as u32);
            gl::ActiveTexture(self.active_texture as GLenum);

            for (param, value) in self.unpack {
                gl::PixelStorei(param, value);
            }
        }
    }
}
