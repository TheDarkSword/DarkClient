//! Centralized visual theme for the in-game overlay.
//!
//! Every color, radius and the egui [`Style`] live here. The rest of the
//! `graphic` module must never hardcode a raw RGB value — pull it from this
//! module so the whole UI stays consistent and re-skinnable from one place.

use egui::{Color32, Context, Margin, Rounding, Stroke, Style, Vec2, Visuals};

// --- Palette ---------------------------------------------------------------

/// Brand color: highlights, enabled modules, headers, focus rings.
pub const ACCENT: Color32 = Color32::from_rgb(38, 198, 156);

/// Backgrounds, darkest to lightest.
pub const BASE: Color32 = Color32::from_rgb(17, 18, 22);
pub const SURFACE: Color32 = Color32::from_rgb(24, 25, 31);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(33, 35, 43);
pub const ELEVATED: Color32 = Color32::from_rgb(29, 31, 38);

/// Borders and separators.
pub const BORDER: Color32 = Color32::from_rgb(42, 44, 54);

/// Text shades, brightest to dimmest.
pub const TEXT: Color32 = Color32::from_rgb(236, 237, 242);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 152, 162);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(92, 94, 104);

/// Status colors for notifications.
pub const WARN: Color32 = Color32::from_rgb(240, 170, 60);
pub const DANGER: Color32 = Color32::from_rgb(226, 74, 74);

// --- Metrics ---------------------------------------------------------------

/// Corner radius for panels.
pub const RADIUS: f32 = 7.0;
/// Corner radius for inner widgets (rows, buttons, cards).
pub const RADIUS_INNER: f32 = 4.0;

// --- Style installation ----------------------------------------------------

/// Builds and installs the overlay style on `ctx`.
///
/// Call exactly once, right after the [`Context`] is created — egui keeps the
/// style in an `Arc`, so re-applying it every frame is pure waste.
pub fn apply(ctx: &Context) {
    let mut style = Style::default();
    let mut v = Visuals::dark();

    v.window_fill = BASE;
    v.panel_fill = BASE;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_rounding = Rounding::same(RADIUS);
    v.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 6.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(140),
    };
    v.popup_shadow = v.window_shadow;

    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::NONE;
    v.hyperlink_color = ACCENT;

    let w = &mut v.widgets;

    w.noninteractive.bg_fill = SURFACE;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);

    w.inactive.bg_fill = SURFACE;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    w.inactive.rounding = Rounding::same(RADIUS_INNER);
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = SURFACE_HOVER;
    w.hovered.weak_bg_fill = SURFACE_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.hovered.rounding = Rounding::same(RADIUS_INNER);
    w.hovered.expansion = 1.0;

    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = ACCENT;
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);
    w.active.rounding = Rounding::same(RADIUS_INNER);
    w.active.expansion = -1.0;

    w.open.bg_fill = SURFACE_HOVER;
    w.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.open.fg_stroke = Stroke::new(1.0_f32, TEXT);

    style.visuals = v;

    style.spacing.item_spacing = Vec2::new(6.0, 6.0);
    style.spacing.window_margin = Margin::same(0.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.interact_size.y = 16.0;
    style.spacing.slider_width = 100.0;

    // A slightly longer animation than egui's default — the module settings
    // panels slide open at this rate, and the brisk default feels abrupt.
    style.animation_time = 0.18;

    ctx.set_style(style);
}

// --- Shared helpers --------------------------------------------------------

/// Linearly interpolates between two colors. `t` is clamped to `0..=1`.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

/// Returns `color` with its alpha scaled by `factor` (`0..=1`).
pub fn with_alpha(color: Color32, factor: f32) -> Color32 {
    let a = (color.a() as f32 * factor.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}
