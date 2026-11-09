use crate::cleanup_client;
use crate::graphic::gl_state::GlStateGuard;
use crate::graphic::input::{GUI_OPEN, MOUSE_STATE, PENDING_EVENTS};
use egui::{ClippedPrimitive, Context, TexturesDelta};
use egui_glow::Painter;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The egui context plus the input state carried between frames. Shared by
/// every graphics backend; only painting differs between them.
pub struct EguiState {
    pub ctx: Context,
    pub last_left_down: bool,
    pub last_right_down: bool,
    pub last_mouse_pos: egui::Pos2,
}

impl EguiState {
    fn new() -> EguiState {
        let ctx = Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        // Install the overlay theme once — egui stores the style in an Arc,
        // so there is no reason to rebuild it every frame.
        crate::graphic::theme::apply(&ctx);
        EguiState {
            ctx,
            last_left_down: false,
            last_right_down: false,
            last_mouse_pos: egui::pos2(0.0, 0.0),
        }
    }
}

static EGUI_STATE: Mutex<Option<EguiState>> = Mutex::new(None);

/// The OpenGL painter, created on the first GL frame.
static GL_PAINTER: Mutex<Option<Painter>> = Mutex::new(None);

/// One overlay frame, laid out and tessellated, ready for a backend to paint.
pub struct EguiFrame {
    pub primitives: Vec<ClippedPrimitive>,
    pub textures_delta: TexturesDelta,
    pub pixels_per_point: f32,
}

/// Monotonic seconds since the overlay first rendered.
///
/// Fed to egui as `RawInput::time` so its clock tracks wall time instead of a
/// fixed predicted delta — the cure for animations stuttering when Minecraft
/// paces frames irregularly (most visibly while the game is paused).
fn elapsed_seconds() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn gather_egui_inputs(
    state: &mut EguiState,
    screen_width: f32,
    screen_height: f32,
    scale_factor: f32,
) -> egui::RawInput {
    let mut raw_input = egui::RawInput {
        time: Some(elapsed_seconds()),
        ..Default::default()
    };

    let viewport_info = egui::ViewportInfo {
        native_pixels_per_point: Some(scale_factor),
        ..Default::default()
    };

    raw_input
        .viewports
        .insert(egui::ViewportId::ROOT, viewport_info);

    let logical_width = screen_width / scale_factor;
    let logical_height = screen_height / scale_factor;

    raw_input.screen_rect = Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(logical_width, logical_height),
    ));

    if !GUI_OPEN.load(Ordering::Relaxed) {
        // Drop any keystrokes captured during the brief window between the GUI
        // closing and the next frame so they cannot leak into the next session.
        if let Ok(mut queue) = PENDING_EVENTS.lock() {
            queue.clear();
        }
        return raw_input;
    }

    // Drain keyboard / text events pushed by the GLFW char + key callbacks.
    if let Ok(mut queue) = PENDING_EVENTS.lock() {
        raw_input.events.append(&mut queue);
    }

    let mut mouse = MOUSE_STATE.lock().unwrap();
    let current_pos = egui::pos2(
        (mouse.x as f32) / scale_factor,
        (mouse.y as f32) / scale_factor,
    );

    if current_pos != state.last_mouse_pos {
        raw_input
            .events
            .push(egui::Event::PointerMoved(current_pos));
        state.last_mouse_pos = current_pos;
    }

    if mouse.left_down != state.last_left_down {
        raw_input.events.push(egui::Event::PointerButton {
            pos: current_pos,
            button: egui::PointerButton::Primary,
            pressed: mouse.left_down,
            modifiers: Default::default(),
        });
        state.last_left_down = mouse.left_down;
    }

    if mouse.right_down != state.last_right_down {
        raw_input.events.push(egui::Event::PointerButton {
            pos: current_pos,
            button: egui::PointerButton::Secondary,
            pressed: mouse.right_down,
            modifiers: Default::default(),
        });
        state.last_right_down = mouse.right_down;
    }

    // Drain the accumulated scroll-wheel delta into an egui event.
    let (scroll_x, scroll_y) = (mouse.scroll_x, mouse.scroll_y);
    mouse.scroll_x = 0.0;
    mouse.scroll_y = 0.0;
    if scroll_x != 0.0 || scroll_y != 0.0 {
        raw_input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(scroll_x, scroll_y),
            modifiers: Default::default(),
        });
    }

    raw_input
}

/// Runs one egui frame for a framebuffer of the given size, in pixels.
pub fn run_frame(screen_width: f32, screen_height: f32) -> EguiFrame {
    let scale_factor = (screen_height / 720.0).max(1.0).floor();

    let mut state_guard = EGUI_STATE.lock().unwrap();
    let state = state_guard.get_or_insert_with(EguiState::new);
    let raw_input = gather_egui_inputs(state, screen_width, screen_height, scale_factor);

    let ctx = state.ctx.clone();
    let full_output = ctx.run(raw_input, |ctx| {
        crate::graphic::gui::render_all(ctx);
    });

    EguiFrame {
        primitives: ctx.tessellate(full_output.shapes, full_output.pixels_per_point),
        textures_delta: full_output.textures_delta,
        pixels_per_point: full_output.pixels_per_point,
    }
}

/// Renders the overlay onto the current OpenGL framebuffer.
///
/// # Safety
/// Must run on the render thread, with Minecraft's GL context current.
pub unsafe fn render_gl() {
    // Everything below — including the painter's first-time setup — changes
    // GL state that Minecraft caches; the guard puts it all back on drop.
    let gl_state = GlStateGuard::capture();
    let viewport = gl_state.viewport();
    let screen_width = viewport[2] as f32;
    let screen_height = viewport[3] as f32;

    let frame = run_frame(screen_width, screen_height);

    let mut painter = GL_PAINTER.lock().unwrap();
    let painter = painter.get_or_insert_with(|| {
        let gl = glow::Context::from_loader_function(|s| {
            crate::graphic::platform::gl_proc_address(s) as *const _
        });
        Painter::new(std::sync::Arc::new(gl), "", None, false).unwrap()
    });

    // Minecraft may leave a pixel-unpack buffer bound and its own unpack
    // parameters set; egui uploads textures from client memory, tightly packed.
    crate::gl::BindBuffer(crate::gl::PIXEL_UNPACK_BUFFER, 0);
    crate::gl::PixelStorei(crate::gl::UNPACK_ALIGNMENT, 1);
    crate::gl::PixelStorei(crate::gl::UNPACK_ROW_LENGTH, 0);
    crate::gl::PixelStorei(crate::gl::UNPACK_SKIP_PIXELS, 0);
    crate::gl::PixelStorei(crate::gl::UNPACK_SKIP_ROWS, 0);

    // Modern Minecraft (Blaze3D, 26.x+) binds GL sampler objects to its
    // texture units. A sampler object *overrides* the texture's own
    // parameters, so a leftover Minecraft sampler — configured for its
    // mipmapped atlas textures — makes egui's mipmap-less font atlas
    // texture-incomplete, and an incomplete texture samples as opaque black.
    // `GlStateGuard::capture` left unit 0 active; egui samples only that one.
    crate::gl::BindSampler(0, 0);

    painter.paint_and_update_textures(
        [screen_width as u32, screen_height as u32],
        frame.pixels_per_point,
        &frame.primitives,
        &frame.textures_delta,
    );

    drop(gl_state);
}

pub fn call_panic() {
    // Persist the user's setup before tearing it down: Panic does not go
    // through the GUI's close handler (which is what normally saves), and the
    // modules are about to be force-disabled for the unload — so save now,
    // while they still hold the state the user actually chose.
    crate::config::save();

    // A module is only running — and so only needs stopping — while active.
    let world_active = crate::state::client().modules.is_active();
    for handle in crate::state::client().modules.handles() {
        let Ok(mut module) = handle.lock() else {
            continue;
        };
        if module.get_module_data().enabled {
            module.get_module_data_mut().set_enabled(false);
            if world_active {
                if let Err(e) = module.on_stop() {
                    log::error!(
                        "Failed to stop module {} on panic: {}",
                        module.get_module_data().name(),
                        e
                    );
                }
            }
        }
    }
    cleanup_client();
}
