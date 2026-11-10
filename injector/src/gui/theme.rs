//! Visual theme — colour palette, widget styling and panel frames.

use egui::{Color32, Context, Frame, Margin, Rounding, Stroke, Vec2};

/// The injector colour palette: a dark base with a single indigo accent.
pub struct Palette;

impl Palette {
    /// Window background.
    pub const BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x1a);
    /// Header / footer panel background.
    pub const PANEL: Color32 = Color32::from_rgb(0x1b, 0x1c, 0x24);
    /// Resting process-card background.
    pub const CARD: Color32 = Color32::from_rgb(0x23, 0x25, 0x2f);
    /// Hovered process-card background.
    pub const CARD_HOVER: Color32 = Color32::from_rgb(0x2c, 0x2e, 0x3b);
    /// Accent — selection, primary action, branding.
    pub const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x6c, 0xf6);
    /// Tinted background of the selected card.
    pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x2c, 0x2a, 0x48);
    /// Primary text.
    pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe7, 0xee);
    /// Secondary / muted text.
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8a, 0x8d, 0x9c);
    /// Success.
    pub const OK: Color32 = Color32::from_rgb(0x4a, 0xd2, 0x95);
    /// In-progress / caution.
    pub const WARN: Color32 = Color32::from_rgb(0xe6, 0xb4, 0x50);
    /// Failure.
    pub const ERR: Color32 = Color32::from_rgb(0xe5, 0x6b, 0x6b);
}

/// Installs the dark theme on the egui context. Called once at startup.
pub fn apply(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.panel_fill = Palette::BG;
    v.window_fill = Palette::BG;
    v.extreme_bg_color = Palette::PANEL;
    v.override_text_color = Some(Palette::TEXT);
    v.selection.bg_fill = Palette::ACCENT.linear_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, Palette::ACCENT);
    v.hyperlink_color = Palette::ACCENT;

    let w = &mut v.widgets;
    w.noninteractive.rounding = Rounding::same(8.0);
    w.inactive.rounding = Rounding::same(8.0);
    w.hovered.rounding = Rounding::same(8.0);
    w.active.rounding = Rounding::same(8.0);
    w.inactive.bg_fill = Palette::CARD;
    w.inactive.weak_bg_fill = Palette::CARD;
    w.inactive.fg_stroke = Stroke::new(1.0_f32, Palette::TEXT);
    w.hovered.bg_fill = Palette::CARD_HOVER;
    w.hovered.weak_bg_fill = Palette::CARD_HOVER;
    w.hovered.fg_stroke = Stroke::new(1.0_f32, Palette::TEXT);
    w.active.bg_fill = Palette::ACCENT;
    w.active.weak_bg_fill = Palette::ACCENT;

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);

    ctx.set_style(style);
}

/// Frame for the top header panel.
pub fn header_frame() -> Frame {
    Frame::none()
        .fill(Palette::PANEL)
        .inner_margin(Margin::symmetric(20.0, 16.0))
}

/// Frame for the bottom footer panel.
pub fn footer_frame() -> Frame {
    Frame::none()
        .fill(Palette::PANEL)
        .inner_margin(Margin::symmetric(16.0, 14.0))
}
