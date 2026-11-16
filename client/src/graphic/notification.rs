//! Toast notifications.
//!
//! `Notification::send` can be called from anywhere; the queue is drained and
//! rendered once per frame by [`draw`]. Cards slide in from the right, stack
//! with a smooth spring, and carry a countdown progress bar.

use crate::graphic::anim::{self, Easing};
use crate::graphic::theme;
use egui::{
    Align2, Color32, Context, FontId, Id, LayerId, Order, Painter, Pos2, Rect, Rounding, Stroke,
    Vec2,
};
use lazy_static::lazy_static;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Severity of a notification — drives its color and icon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotificationType {
    // Only warnings are sent so far.
    #[allow(dead_code)]
    Info,
    Warning,
    #[allow(dead_code)]
    Alert,
}

impl NotificationType {
    /// Accent color for this severity.
    fn color(self) -> Color32 {
        match self {
            NotificationType::Info => theme::ACCENT,
            NotificationType::Warning => theme::WARN,
            NotificationType::Alert => theme::DANGER,
        }
    }

    /// Single-glyph icon for this severity.
    fn icon(self) -> &'static str {
        match self {
            NotificationType::Info => "i",
            NotificationType::Warning => "!",
            NotificationType::Alert => "x",
        }
    }
}

/// A single queued toast.
pub struct Notification {
    /// Stable id, used to keep per-card animations consistent across frames.
    id: u64,
    notif_type: NotificationType,
    title: String,
    message: String,
    spawn_time: Instant,
    duration: f32,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

lazy_static! {
    static ref QUEUE: Mutex<Vec<Notification>> = Mutex::new(Vec::new());
}

impl Notification {
    /// Queues a notification with the default 3-second lifetime.
    pub fn send(notif_type: NotificationType, title: &str, message: &str) {
        Self::send_with_time(notif_type, title, message, 3.0);
    }

    /// Queues a notification that lives for `duration` seconds.
    pub fn send_with_time(notif_type: NotificationType, title: &str, message: &str, duration: f32) {
        QUEUE.lock().unwrap().push(Notification {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            notif_type,
            title: title.to_string(),
            message: message.to_string(),
            spawn_time: Instant::now(),
            duration,
        });
    }
}

/// Card geometry.
const WIDTH: f32 = 258.0;
const HEIGHT: f32 = 58.0;
const MARGIN: f32 = 14.0;
const GAP: f32 = 8.0;
/// Duration of the slide-in / slide-out transition, in seconds.
const SLIDE: f32 = 0.28;

/// Drains expired toasts and draws the rest. Call once per frame.
pub fn draw(ctx: &Context) {
    let mut queue = QUEUE.lock().unwrap();
    let now = Instant::now();
    queue.retain(|n| now.duration_since(n.spawn_time).as_secs_f32() < n.duration);
    if queue.is_empty() {
        return;
    }

    // One painter for every card — the old code rebuilt it seven times each.
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("notifications")));
    let screen_w = ctx.screen_rect().width();

    for (index, n) in queue.iter().enumerate() {
        let elapsed = now.duration_since(n.spawn_time).as_secs_f32();
        let remaining = n.duration - elapsed;

        // Horizontal travel: 0 = docked, 1 = fully off the right edge.
        let offset = if elapsed < SLIDE {
            1.0 - Easing::Out.apply(elapsed / SLIDE)
        } else if remaining < SLIDE {
            1.0 - Easing::In.apply(remaining / SLIDE)
        } else {
            0.0
        };

        // Smooth vertical stacking so cards glide up as others expire.
        let target_y = MARGIN + index as f32 * (HEIGHT + GAP);
        let y = anim::ease_to(
            ctx,
            Id::new("notif_y").with(n.id),
            target_y,
            0.2,
            Easing::Out,
        );

        let x = (screen_w - MARGIN - WIDTH) + (WIDTH + MARGIN) * offset;
        let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(WIDTH, HEIGHT));
        draw_card(&painter, rect, n, remaining);
    }
}

/// Paints one toast card into `rect`.
fn draw_card(painter: &Painter, rect: Rect, n: &Notification, remaining: f32) {
    let accent = n.notif_type.color();
    let radius = Rounding::same(theme::RADIUS_INNER);

    painter.rect_filled(
        rect,
        radius,
        Color32::from_rgba_unmultiplied(16, 17, 21, 240),
    );
    painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, theme::BORDER));

    // Accent rail down the left edge.
    let rail = Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height()));
    painter.rect_filled(
        rail,
        Rounding {
            nw: theme::RADIUS_INNER,
            sw: theme::RADIUS_INNER,
            ne: 0.0,
            se: 0.0,
        },
        accent,
    );

    // Icon disc.
    let icon_center = Pos2::new(rect.min.x + 30.0, rect.center().y);
    painter.circle_filled(icon_center, 13.0, theme::with_alpha(accent, 0.18));
    painter.text(
        icon_center,
        Align2::CENTER_CENTER,
        n.notif_type.icon(),
        FontId::proportional(15.0),
        accent,
    );

    // Title and message.
    let text_x = rect.min.x + 52.0;
    painter.text(
        Pos2::new(text_x, rect.min.y + 13.0),
        Align2::LEFT_TOP,
        &n.title,
        FontId::proportional(14.0),
        theme::TEXT,
    );
    painter.text(
        Pos2::new(text_x, rect.min.y + 31.0),
        Align2::LEFT_TOP,
        &n.message,
        FontId::proportional(12.0),
        theme::TEXT_DIM,
    );

    // Countdown progress bar pinned to the bottom edge.
    let fraction = (remaining / n.duration).clamp(0.0, 1.0);
    let bar = Rect::from_min_size(
        rect.left_bottom() - Vec2::new(0.0, 3.0),
        Vec2::new(rect.width() * fraction, 3.0),
    );
    painter.rect_filled(
        bar,
        Rounding {
            nw: 0.0,
            ne: 0.0,
            sw: theme::RADIUS_INNER,
            se: 0.0,
        },
        theme::with_alpha(accent, 0.85),
    );
}
