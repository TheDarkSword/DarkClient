//! egui front-end for the injector.
//!
//! Thin layer over [`InjectorApp`]: it renders the app's state and forwards
//! user actions. All injection logic lives in `app`/`inject`.

mod theme;
mod widgets;

use eframe::{Frame, NativeOptions};
use egui::{Align, Context, Layout, RichText, ScrollArea, ViewportBuilder};

use crate::app::InjectorApp;

/// Launches the GUI. Blocks until the window is closed.
pub fn run() -> Result<(), eframe::Error> {
    let native_options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([480.0, 520.0])
            .with_min_inner_size([380.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DarkClient Injector",
        native_options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let mut app = InjectorApp::new();
            app.scan();
            Ok(Box::new(InjectorGui { app }))
        }),
    )
}

/// The eframe application — owns the [`InjectorApp`] and renders it.
struct InjectorGui {
    app: InjectorApp,
}

impl eframe::App for InjectorGui {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        self.app.poll();

        egui::TopBottomPanel::top("header")
            .frame(theme::header_frame())
            .show(ctx, widgets::header);

        egui::TopBottomPanel::bottom("footer")
            .frame(theme::footer_frame())
            .show(ctx, |ui| {
                widgets::status_banner(ui, self.app.status());
                ui.add_space(6.0);
                let mut force = self.app.force();
                if ui
                    .checkbox(&mut force, "Ignore version check (development)")
                    .on_hover_text(
                        "Inject even into a Minecraft version DarkClient does not support",
                    )
                    .changed()
                {
                    self.app.set_force(force);
                }
                ui.add_space(6.0);
                if widgets::inject_button(ui, &self.app).clicked() {
                    self.app.start_injection();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| self.process_list(ui));

        // Keep repainting while a worker thread runs so its result and the
        // status spinner stay live.
        if self.app.is_busy() {
            ctx.request_repaint();
        }
    }
}

impl InjectorGui {
    /// Renders the scan row and the scrollable list of process cards.
    fn process_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::tool_button(ui, "🔄  Scan", !self.app.is_busy()).clicked() {
                self.app.scan();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} found", self.app.processes().len()))
                        .color(theme::Palette::TEXT_DIM),
                );
            });
        });
        ui.add_space(8.0);

        if self.app.processes().is_empty() {
            widgets::empty_state(ui);
            return;
        }

        let selected = self.app.selected_pid();
        let mut clicked = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for proc in self.app.processes() {
                    let is_selected = selected == Some(proc.pid);
                    if widgets::process_card(ui, proc, is_selected).clicked() {
                        clicked = Some(proc.pid);
                    }
                    ui.add_space(6.0);
                }
            });

        if let Some(pid) = clicked {
            self.app.select(pid);
        }
    }
}
