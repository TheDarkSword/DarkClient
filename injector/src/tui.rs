//! Terminal fallback front-end (`--tui`), built on the shared [`InjectorApp`].

use std::io::{stdout, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};

use crate::app::{InjectionStatus, InjectorApp};

/// How long each loop iteration waits for a key before redrawing — also the
/// cadence at which an in-flight injection result is picked up.
const POLL_INTERVAL: Duration = Duration::from_millis(120);

/// Runs the text UI until the user quits.
pub fn run_tui() {
    let mut out = stdout();
    let _ = execute!(out, EnterAlternateScreen, cursor::Hide);

    let mut app = InjectorApp::new();
    app.scan();
    let mut cursor_row = 0usize;

    loop {
        app.poll();
        clamp_cursor(&app, &mut cursor_row);
        render(&mut out, &app, cursor_row);

        match read_key(POLL_INTERVAL) {
            Some(KeyCode::Char('q')) | Some(KeyCode::Esc) => break,
            Some(KeyCode::Char('s')) | Some(KeyCode::Char('r')) => {
                app.scan();
                cursor_row = 0;
            }
            Some(KeyCode::Char('f')) => app.set_force(!app.force()),
            Some(KeyCode::Up) => cursor_row = cursor_row.saturating_sub(1),
            Some(KeyCode::Down) => {
                let last = app.processes().len().saturating_sub(1);
                cursor_row = (cursor_row + 1).min(last);
            }
            Some(KeyCode::Enter) => {
                if let Some(proc) = app.processes().get(cursor_row) {
                    app.select(proc.pid);
                    app.start_injection();
                }
            }
            _ => {}
        }
    }

    let _ = execute!(out, LeaveAlternateScreen, cursor::Show);
    println!("Exited DarkClient Injector.");
}

/// Keeps the highlighted row within the current process list.
fn clamp_cursor(app: &InjectorApp, cursor_row: &mut usize) {
    let len = app.processes().len();
    *cursor_row = (*cursor_row).min(len.saturating_sub(1));
}

/// Draws the whole screen.
fn render(out: &mut Stdout, app: &InjectorApp, cursor_row: usize) {
    let _ = execute!(out, Clear(ClearType::All), cursor::MoveTo(0, 0));

    println!("=== DarkClient Injector (TUI) ===");
    println!();

    if app.processes().is_empty() {
        println!("  No Minecraft instances found.");
    } else {
        println!("  Up/Down to choose, Enter to inject:");
        println!();
        for (row, proc) in app.processes().iter().enumerate() {
            if row == cursor_row {
                let _ = execute!(out, SetForegroundColor(Color::Green));
                println!("  > PID {} — {}", proc.pid, proc.info);
                let _ = execute!(out, ResetColor);
            } else {
                println!("    PID {} — {}", proc.pid, proc.info);
            }
        }
    }

    println!();
    let (color, line) = status_line(app.status());
    let _ = execute!(out, SetForegroundColor(color));
    println!("  {line}");
    let _ = execute!(out, ResetColor);

    println!();
    let force = if app.force() { "on" } else { "off" };
    println!("  [s] scan   [Enter] inject   [f] ignore version check: {force}   [q] quit");
}

/// Maps an [`InjectionStatus`] to a terminal colour and message.
fn status_line(status: &InjectionStatus) -> (Color, String) {
    let color = match status {
        InjectionStatus::Idle => Color::Grey,
        InjectionStatus::Scanning => Color::Cyan,
        InjectionStatus::Injecting(_) => Color::Yellow,
        InjectionStatus::Done { .. } => Color::Green,
        InjectionStatus::Failed(_) => Color::Red,
    };
    (color, status.message())
}

/// Waits up to `timeout` for a key press, returning its code if one arrived.
fn read_key(timeout: Duration) -> Option<KeyCode> {
    if event::poll(timeout).ok()? {
        if let Ok(Event::Key(key)) = event::read() {
            return Some(key.code);
        }
    }
    None
}
