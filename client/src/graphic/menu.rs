//! The ClickGUI: a draggable, spring-animated panel per module category.
//!
//! The menu is split into small, single-purpose functions so the drawing
//! code reads top-down. Module mutexes are locked **exactly once** per module
//! per frame; all motion goes through [`anim`], so it is frame-rate
//! independent and needs no per-widget state threaded through the call tree.

use crate::graphic::anim::{self, Easing, SpringCfg};
use crate::graphic::input::LAST_KEY_PRESSED;
use crate::graphic::notification::{Notification, NotificationType};
use crate::graphic::theme;
use crate::module::{KeyboardKey, ModuleCategory, ModuleData, ModuleId, ModuleSetting, ModuleType};
use egui::{
    Align, Align2, Button, Color32, Context, FontId, Id, LayerId, Layout, Margin, Order, Painter,
    Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke, Ui, Vec2,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Width of a category panel.
const PANEL_W: f32 = 168.0;
/// Horizontal gap between panels in the auto-layout grid.
const GAP: f32 = 14.0;
/// Height of a panel's draggable title bar.
const TITLE_H: f32 = 30.0;
/// Height of a single module row.
const ROW_H: f32 = 24.0;
/// Vertical distance between grid rows when panels wrap.
const ROW_STRIDE: f32 = 320.0;
/// Top-left corner of the first panel slot.
const ORIGIN_X: f32 = 40.0;
const ORIGIN_Y: f32 = 58.0;

/// Whether a color picker popup was open last frame — a pasted color then
/// goes to it whole, instead of into a single R / G / B field.
static COLOR_PICKER_OPEN: AtomicBool = AtomicBool::new(false);
/// A pasted color waiting for the open picker to take it.
static PASTED_COLOR: Mutex<Option<[u8; 4]>> = Mutex::new(None);

/// Offers text being pasted into the overlay. When a color picker is open and
/// `text` is a whole color, the picker takes it and this returns `true`: the
/// paste must not also reach the focused field.
pub fn offer_color_paste(text: &str) -> bool {
    if !COLOR_PICKER_OPEN.load(Ordering::Relaxed) {
        return false;
    }
    let Some(color) = crate::graphic::color_text::parse_color(text) else {
        return false;
    };
    if let Ok(mut pasted) = PASTED_COLOR.lock() {
        *pasted = Some(color);
    }
    true
}

/// A shared handle to one module.
type ModuleArc = Arc<Mutex<ModuleType>>;
/// The whole module registry, as borrowed from the read guard.
type ModuleMap = HashMap<ModuleId, ModuleArc>;

/// Draws the entire ClickGUI. `progress` is the 0..1 open animation factor.
pub fn draw(ctx: &Context, progress: f32) {
    COLOR_PICKER_OPEN.store(false, Ordering::Relaxed);
    draw_backdrop(ctx, progress);

    let registry = crate::state::client().modules.by_id();

    // Single lock per module: collect the data layout needs, nothing more.
    let mut entries: Vec<(ModuleId, ModuleCategory)> = registry
        .iter()
        .map(|(id, arc)| {
            let module = arc.lock().unwrap();
            (*id, module.get_module_data().category)
        })
        .collect();
    entries.sort_by_key(|(id, _)| id.display_name());

    draw_toolbar(ctx, progress);

    // Auto-layout: place non-empty categories left-to-right, wrapping rows.
    let categories = [
        ModuleCategory::Combat,
        ModuleCategory::Movement,
        ModuleCategory::Render,
        ModuleCategory::Player,
        ModuleCategory::World,
        ModuleCategory::Misc,
    ];
    let screen_w = ctx.screen_rect().width();
    let mut slot = Pos2::new(ORIGIN_X, ORIGIN_Y);

    for category in categories {
        let members: Vec<(String, &ModuleArc)> = entries
            .iter()
            .filter(|(_, cat)| *cat == category)
            .filter_map(|(id, _)| {
                registry
                    .get(id)
                    .map(|arc| (id.display_name().to_string(), arc))
            })
            .collect();
        if members.is_empty() {
            continue;
        }

        if slot.x + PANEL_W > screen_w - 20.0 && slot.x > ORIGIN_X {
            slot.x = ORIGIN_X;
            slot.y += ROW_STRIDE;
        }

        draw_panel(ctx, progress, category, &members, slot, &registry);
        slot.x += PANEL_W + GAP;
    }
}

/// Dims the game behind the menu, fading with the open animation.
fn draw_backdrop(ctx: &Context, progress: f32) {
    let painter = ctx.layer_painter(LayerId::new(Order::Middle, Id::new("clickgui_backdrop")));
    let alpha = (170.0 * progress) as u8;
    painter.rect_filled(
        ctx.screen_rect(),
        Rounding::ZERO,
        Color32::from_black_alpha(alpha),
    );
}

/// Top-center bar: the brand title plus the Panic / Reset actions.
fn draw_toolbar(ctx: &Context, progress: f32) {
    let slide = 16.0 * (1.0 - progress);
    egui::Area::new(Id::new("clickgui_toolbar"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 12.0 - slide))
        .show(ctx, |ui| {
            ui.set_opacity(progress);
            egui::Frame::none()
                .fill(theme::BASE)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .rounding(Rounding::same(theme::RADIUS))
                .inner_margin(Margin::symmetric(12.0, 7.0))
                .shadow(ui.style().visuals.window_shadow)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("Dark").size(15.0).strong().color(theme::TEXT));
                        ui.label(
                            RichText::new("Client")
                                .size(15.0)
                                .strong()
                                .color(theme::ACCENT),
                        );
                        ui.add_space(16.0);

                        let panic =
                            Button::new(RichText::new("Panic").size(12.5).color(theme::DANGER))
                                .fill(theme::ELEVATED);
                        if ui.add(panic).clicked() {
                            std::thread::spawn(crate::graphic::ui_engine::call_panic);
                        }

                        ui.add_space(6.0);
                        let reset_ui = Button::new(
                            RichText::new("Reset UI").size(12.5).color(theme::TEXT_DIM),
                        )
                        .fill(theme::ELEVATED);
                        if ui.add(reset_ui).clicked() {
                            // Drop the saved layout — panels spring home,
                            // modules collapse — then persist the reset.
                            ctx.memory_mut(|mem| mem.reset_areas());
                            ctx.data_mut(|data| data.clear());
                            crate::config::reset_ui_state();
                            crate::config::save();
                        }

                        ui.add_space(6.0);
                        let reset_settings = Button::new(
                            RichText::new("Reset Settings")
                                .size(12.5)
                                .color(theme::TEXT_DIM),
                        )
                        .fill(theme::ELEVATED);
                        if ui.add(reset_settings).clicked() {
                            // Restore factory defaults, then persist them.
                            crate::state::client().modules.reset_settings();
                            crate::config::save();
                        }
                    });
                });
        });
}

/// Draws one category panel: spring-positioned, draggable, with its rows.
fn draw_panel(
    ctx: &Context,
    progress: f32,
    category: ModuleCategory,
    members: &[(String, &ModuleArc)],
    slot: Pos2,
    registry: &ModuleMap,
) {
    let name = category.display_name();

    // The panel's drag target lives in the config (persisted); the spring
    // smooths the rendered position toward it, frame-rate independently. A
    // panel the user has never moved sits at its auto-layout `slot`.
    let target = crate::config::panel_pos(category)
        .map(|p| Pos2::new(p[0], p[1]))
        .unwrap_or(slot);
    let pos = anim::spring_pos(
        ctx,
        Id::new("panel_pos").with(name),
        target,
        SpringCfg::PANEL,
    );

    // Spawn animation: slide the panel up into place as the menu opens.
    let render_pos = pos + Vec2::new(0.0, 16.0 * (1.0 - progress));

    egui::Area::new(Id::new("clickgui_panel").with(name))
        .current_pos(render_pos)
        .order(Order::Foreground)
        .show(ctx, |ui| {
            ui.set_opacity(progress);
            egui::Frame::none()
                .fill(theme::BASE)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .rounding(Rounding::same(theme::RADIUS))
                .shadow(ui.style().visuals.window_shadow)
                .show(ui, |ui| {
                    // All sizing constraints first: `set_max_*` re-anchors the
                    // layout cursor, so anything drawn beforehand would be
                    // overwritten at the panel's top edge.
                    ui.set_min_width(PANEL_W);
                    ui.set_max_width(PANEL_W);
                    // Cap the panel at the window height. The ui is also given
                    // room to grow into: an `Area` otherwise offers its content
                    // only the previous frame's size, which would pin the
                    // ScrollArea — and the panel — to its collapsed height. The
                    // Frame still shrinks to the actual content.
                    let max_rows_h =
                        (ctx.screen_rect().height() - render_pos.y - TITLE_H - 16.0).max(ROW_H);
                    ui.set_max_height(max_rows_h + TITLE_H + 8.0);

                    draw_title_bar(ui, category);

                    // Floating scroll bar, kept slim even when hovered —
                    // egui's default expands it to an unsightly width.
                    let mut scroll = egui::style::ScrollStyle::floating();
                    scroll.bar_width = 5.0;
                    ui.style_mut().spacing.scroll = scroll;
                    // A floating handle borrows `active.fg_stroke` for its
                    // colour; tint that with the accent so a dragged bar shows
                    // the brand colour instead of the theme's black.
                    let active_fg = ui.visuals().widgets.active.fg_stroke.color;
                    ui.visuals_mut().widgets.active.fg_stroke.color = theme::ACCENT;
                    egui::ScrollArea::vertical()
                        .id_salt(name)
                        .max_height(max_rows_h)
                        .auto_shrink([false, true])
                        .drag_to_scroll(false)
                        .show(ui, |ui| {
                            // Restore the normal active foreground for the
                            // panel's own widgets.
                            ui.visuals_mut().widgets.active.fg_stroke.color = active_fg;
                            ui.set_min_width(PANEL_W);
                            ui.set_max_width(PANEL_W);
                            for (module_name, arc) in members {
                                draw_module_row(ui, module_name, arc, registry);
                            }
                            ui.add_space(6.0);
                        });
                });
        });
}

/// Draggable title bar with the category name and an accent underline.
fn draw_title_bar(ui: &mut Ui, category: ModuleCategory) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(PANEL_W, TITLE_H), Sense::drag());
    if response.dragged() {
        let delta = ui.ctx().input(|i| i.pointer.delta());
        let current = crate::config::panel_pos(category)
            .map(|p| Pos2::new(p[0], p[1]))
            .unwrap_or(rect.min);
        let next = current + delta;
        crate::config::set_panel_pos(category, [next.x, next.y]);
    }

    let painter = ui.painter();
    painter.rect_filled(
        rect,
        Rounding {
            nw: theme::RADIUS,
            ne: theme::RADIUS,
            sw: 0.0,
            se: 0.0,
        },
        theme::ELEVATED,
    );
    painter.text(
        rect.left_center() + Vec2::new(12.0, 0.0),
        Align2::LEFT_CENTER,
        category.display_name(),
        FontId::proportional(13.5),
        theme::TEXT,
    );
    let underline = Rect::from_min_size(
        rect.left_bottom() - Vec2::new(0.0, 2.0),
        Vec2::new(PANEL_W, 2.0),
    );
    painter.rect_filled(underline, Rounding::ZERO, theme::ACCENT);
}

/// Draws a single module row and its (optional) expandable settings panel.
fn draw_module_row(ui: &mut Ui, name: &str, arc: &ModuleArc, registry: &ModuleMap) {
    let mut module = arc.lock().unwrap();
    let enabled = module.get_module_data().enabled;
    let id = module.get_module_data().id;

    let (rect, response) = ui.allocate_exact_size(Vec2::new(PANEL_W, ROW_H), Sense::click());
    let arrow_zone = Rect::from_min_size(
        Pos2::new(rect.max.x - 24.0, rect.min.y),
        Vec2::new(24.0, ROW_H),
    );
    // The chevron is its own click target, layered over the row: clicking it
    // expands the module, clicking anywhere else on the row toggles it.
    let arrow_response = ui.interact(arrow_zone, Id::new("row_arrow").with(name), Sense::click());

    // egui's CollapsingState animates the settings panel open and closed — a
    // smooth, native slide. Its open flag is kept in the config (persisted),
    // not in egui's own store.
    let expanded = crate::config::is_expanded(id);
    let mut collapse = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        Id::new("row_collapse").with(name),
        expanded,
    );

    let ctx = ui.ctx();
    let hover = anim::toggle(
        ctx,
        Id::new("row_hov").with(name),
        response.hovered(),
        0.12,
        Easing::Out,
    );
    let enable = anim::toggle(
        ctx,
        Id::new("row_en").with(name),
        enabled,
        0.18,
        Easing::Out,
    );

    // --- paint base row ---
    {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            Rounding::ZERO,
            theme::lerp_color(Color32::TRANSPARENT, theme::SURFACE_HOVER, hover),
        );
        if enable > 0.001 {
            let bar = Rect::from_center_size(
                Pos2::new(rect.min.x + 1.5, rect.center().y),
                Vec2::new(3.0, ROW_H * enable),
            );
            painter.rect_filled(bar, Rounding::ZERO, theme::ACCENT);
        }
        painter.text(
            Pos2::new(rect.min.x + 12.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            theme::lerp_color(theme::TEXT_DIM, theme::TEXT, hover.max(enable)),
        );
    }

    // --- interaction ---
    // The chevron — or a right-click anywhere on the row — expands the module;
    // a left-click on the rest of the row toggles it on/off.
    if arrow_response.clicked() || response.secondary_clicked() {
        crate::config::set_expanded(id, !expanded);
    } else if response.clicked() {
        let next = !enabled;
        module.get_module_data_mut().set_enabled(next);
        // Start/stop immediately only when in a world; at the menu the flag
        // just arms the module for the next world entry.
        if crate::state::client().modules.is_active() {
            let _ = if next {
                module.on_start()
            } else {
                module.on_stop()
            };
        }
    }

    // --- chevron + settings ---
    // Re-sync the CollapsingState to the persisted flag — it may have just
    // flipped above, or been changed by a config load / "Reset UI".
    collapse.set_open(crate::config::is_expanded(id));
    let openness = collapse.openness(ui.ctx());
    let chevron_color = if arrow_response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    paint_chevron(ui.painter(), arrow_zone.center(), openness, chevron_color);

    // The body slides open/closed; `CollapsingState` clips it to the animated
    // height, so the panel grows and shrinks smoothly.
    collapse.show_body_unindented(ui, |ui| {
        draw_settings(ui, module.get_module_data_mut(), arc, registry);
    });
    collapse.store(ui.ctx());
}

/// Draws a chevron that rotates from ▸ (collapsed) to ▾ (expanded).
fn paint_chevron(painter: &Painter, center: Pos2, open: f32, color: Color32) {
    const S: f32 = 3.6;
    let angle = open.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let rotate = |v: Vec2| Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    let points = [
        Vec2::new(S, 0.0),
        Vec2::new(-S * 0.7, -S),
        Vec2::new(-S * 0.7, S),
    ]
    .into_iter()
    .map(|v| center + rotate(v))
    .collect::<Vec<_>>();
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Renders the keybind row and every [`ModuleSetting`] of an expanded module.
fn draw_settings(ui: &mut Ui, data: &mut ModuleData, arc: &ModuleArc, registry: &ModuleMap) {
    egui::Frame::none()
        .fill(theme::SURFACE)
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(PANEL_W - 20.0);
            ui.set_max_width(PANEL_W - 20.0);
            ui.spacing_mut().item_spacing.y = 7.0;

            keybind_row(ui, data, arc, registry);

            for setting in &mut data.settings {
                match setting {
                    ModuleSetting::Toggle { name, value } => {
                        ui.checkbox(value, label(name));
                    }
                    ModuleSetting::Slider {
                        name,
                        value,
                        min,
                        max,
                    } => {
                        labelled_value(ui, name, &format!("{value:.2}"));
                        ui.spacing_mut().slider_width = ui.available_width();
                        ui.add(egui::Slider::new(value, *min..=*max).show_value(false));
                    }
                    ModuleSetting::Choice {
                        name,
                        value,
                        options,
                    } => {
                        ui.label(label(name));
                        egui::ComboBox::from_id_salt(Id::new("choice").with(name.as_str()))
                            .width(ui.available_width())
                            .selected_text(
                                RichText::new(
                                    options.get(*value).map(String::as_str).unwrap_or("—"),
                                )
                                .size(12.0),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, option) in options.iter().enumerate() {
                                    ui.selectable_value(value, idx, option.as_str());
                                }
                            });
                    }
                    ModuleSetting::Color { name, value, alpha } => {
                        ui.horizontal(|ui| {
                            ui.label(label(name));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                // Edited as sRGB — the space the values are drawn
                                // in — and read back unmultiplied, so the swatch
                                // matches what is drawn and alpha never darkens
                                // the color.
                                // The id egui's color button gives its popup.
                                let popup = ui.auto_id_with("popup");
                                if ui.memory(|memory| memory.is_popup_open(popup)) {
                                    COLOR_PICKER_OPEN.store(true, Ordering::Relaxed);
                                    let pasted =
                                        PASTED_COLOR.lock().ok().and_then(|mut p| p.take());
                                    if let Some([r, g, b, a]) = pasted {
                                        let a = if *alpha { a } else { 255 };
                                        *value = [r, g, b, a].map(|c| c as f32 / 255.0);
                                    }
                                }
                                let [r, g, b, a] = value.map(|c| (c * 255.0).round() as u8);
                                let mut color = Color32::from_rgba_unmultiplied(r, g, b, a);
                                let mode = if *alpha {
                                    egui::color_picker::Alpha::OnlyBlend
                                } else {
                                    egui::color_picker::Alpha::Opaque
                                };
                                if egui::color_picker::color_edit_button_srgba(ui, &mut color, mode)
                                    .changed()
                                {
                                    let [r, g, b, a] = color.to_srgba_unmultiplied();
                                    let a = if *alpha { a } else { 255 };
                                    if a == 0 {
                                        // Fully transparent premultiplied color has
                                        // no RGB left: keep the stored one.
                                        value[3] = 0.0;
                                    } else {
                                        *value = [r, g, b, a].map(|c| c as f32 / 255.0);
                                    }
                                }
                            });
                        });
                    }
                }
            }
            ui.add_space(1.0);
        });
}

/// The "Bind" row: click the button, then press a key (Esc unbinds).
fn keybind_row(ui: &mut Ui, data: &mut ModuleData, arc: &ModuleArc, registry: &ModuleMap) {
    ui.horizontal(|ui| {
        ui.label(label("Bind"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let bind_id = Id::new("kb_listen").with(data.name());
            let listening = ui.data(|d| d.get_temp::<bool>(bind_id).unwrap_or(false));

            let caption = if listening {
                if capture_keybind(data, arc, registry) {
                    ui.data_mut(|d| d.insert_temp(bind_id, false));
                }
                "press…".to_string()
            } else {
                data.key_bind.to_string()
            };

            let color = if listening {
                theme::ACCENT
            } else {
                theme::TEXT_DIM
            };
            let button = Button::new(RichText::new(caption).size(12.0).color(color))
                .fill(theme::ELEVATED)
                .stroke(Stroke::NONE);
            if ui.add(button).clicked() {
                let next = !listening;
                ui.data_mut(|d| d.insert_temp(bind_id, next));
                if next {
                    LAST_KEY_PRESSED.store(-1, Ordering::Relaxed);
                }
            }
        });
    });
}

/// Consumes the latest key press while in bind mode.
///
/// Returns `true` when listening should stop — a key was applied, the module
/// was unbound, or the chosen key was rejected as a duplicate.
fn capture_keybind(data: &mut ModuleData, arc: &ModuleArc, registry: &ModuleMap) -> bool {
    let pressed = LAST_KEY_PRESSED.swap(-1, Ordering::Relaxed);
    if pressed == -1 {
        return false;
    }
    let key = KeyboardKey::from(pressed);

    if key == KeyboardKey::KeyEscape {
        data.key_bind = KeyboardKey::KeyNone;
        return true;
    }

    // Reject keys already taken by another module.
    for other in registry.values() {
        if Arc::ptr_eq(arc, other) {
            continue;
        }
        let owner = other.lock().unwrap();
        if owner.get_module_data().key_bind == key {
            let owner_name = owner.get_module_data().name().to_string();
            drop(owner);
            Notification::send(
                NotificationType::Warning,
                "Keybind in use",
                &format!("'{owner_name}' is bound to {key}"),
            );
            return true;
        }
    }

    data.key_bind = key;
    true
}

/// A dimmed 12px setting label.
fn label(text: &str) -> RichText {
    RichText::new(text).size(12.0).color(theme::TEXT_DIM)
}

/// Draws a `name … value` row, value accented and right-aligned.
fn labelled_value(ui: &mut Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(label(name));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).color(theme::ACCENT));
        });
    });
}
