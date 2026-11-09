//! Reusable egui widgets for the injector GUI.

use egui::{
    pos2, Align2, Button, Color32, FontId, Frame, Margin, Response, RichText, Sense, Spinner,
    Stroke, Ui, Vec2,
};

use super::theme::Palette;
use crate::app::{InjectionStatus, InjectorApp};
use crate::platform::ProcessInfo;

/// Branding block rendered in the top header panel.
pub fn header(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("DarkClient")
                .size(22.0)
                .strong()
                .color(Palette::TEXT),
        );
        ui.label(
            RichText::new("INJECTOR")
                .size(12.0)
                .strong()
                .color(Palette::ACCENT),
        );
    });
    ui.label(
        RichText::new("Pick a Minecraft instance and inject the client.")
            .size(12.0)
            .color(Palette::TEXT_DIM),
    );
}

/// A small secondary button (used for "Scan").
pub fn tool_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let button = Button::new(RichText::new(label).color(Palette::TEXT))
        .fill(Palette::CARD)
        .min_size(Vec2::new(0.0, 32.0));
    ui.add_enabled(enabled, button)
}

/// A selectable process card. Returns the click response.
pub fn process_card(ui: &mut Ui, proc: &ProcessInfo, selected: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::click());

    let bg = if selected {
        Palette::ACCENT_SOFT
    } else if response.hovered() {
        Palette::CARD_HOVER
    } else {
        Palette::CARD
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, bg);
    if selected {
        painter.rect_stroke(rect, 10.0, Stroke::new(1.5, Palette::ACCENT));
    }

    let dot_color = if selected {
        Palette::ACCENT
    } else {
        Palette::TEXT_DIM
    };
    painter.circle_filled(rect.left_center() + Vec2::new(18.0, 0.0), 4.0, dot_color);

    let text_x = rect.left() + 34.0;
    painter.text(
        pos2(text_x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        format!("PID {}", proc.pid),
        FontId::proportional(15.0),
        Palette::TEXT,
    );
    painter.text(
        pos2(text_x, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        &proc.info,
        FontId::proportional(12.0),
        Palette::TEXT_DIM,
    );

    response
}

/// Placeholder shown when no Minecraft processes were found.
pub fn empty_state(ui: &mut Ui) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("No Minecraft instances found")
                .size(14.0)
                .color(Palette::TEXT_DIM),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Start the game, then press Scan.")
                .size(12.0)
                .color(Palette::TEXT_DIM),
        );
    });
}

/// The status banner shown in the footer.
pub fn status_banner(ui: &mut Ui, status: &InjectionStatus) {
    let (color, icon) = match status {
        InjectionStatus::Idle => (Palette::TEXT_DIM, "●"),
        InjectionStatus::Scanning => (Palette::ACCENT, "◌"),
        InjectionStatus::Injecting(_) => (Palette::WARN, "◌"),
        InjectionStatus::Done { .. } => (Palette::OK, "✔"),
        InjectionStatus::Failed(_) => (Palette::ERR, "✖"),
    };
    let busy = matches!(
        status,
        InjectionStatus::Scanning | InjectionStatus::Injecting(_)
    );

    Frame::none()
        .fill(Palette::BG)
        .rounding(8.0)
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if busy {
                    ui.add(Spinner::new().size(14.0).color(color));
                } else {
                    ui.label(RichText::new(icon).size(14.0).color(color));
                }
                ui.label(RichText::new(status.message()).color(Palette::TEXT));
            });
        });
}

/// The full-width primary "Inject" button.
pub fn inject_button(ui: &mut Ui, app: &InjectorApp) -> Response {
    let enabled = app.selected_pid().is_some() && !app.is_busy();
    let label = if app.is_busy() {
        "Injecting…"
    } else {
        "Inject Client"
    };
    let button = Button::new(
        RichText::new(label)
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(Palette::ACCENT)
    .min_size(Vec2::new(ui.available_width(), 42.0));
    ui.add_enabled(enabled, button)
}
