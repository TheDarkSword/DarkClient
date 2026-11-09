//! UI-agnostic core shared by the GUI and the TUI front-ends.
//!
//! Both front-ends own an [`InjectorApp`], drive it, and only render its
//! state. Injection runs on a worker thread so neither front-end blocks.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use crate::inject;
use crate::platform::{find_minecraft_processes, InjectError, ProcessInfo};

/// Where an injection attempt currently stands.
#[derive(Debug, Clone)]
pub enum InjectionStatus {
    /// Idle and ready.
    Idle,
    /// A process scan is running.
    Scanning,
    /// An injection into the given pid is in progress.
    Injecting(u32),
    /// The last injection into `pid` succeeded; the client runs on
    /// Minecraft `version`.
    Done { pid: u32, version: String },
    /// The last action failed, with a human-readable reason.
    Failed(String),
}

impl InjectionStatus {
    /// Short human-readable line suitable for a status bar.
    pub fn message(&self) -> String {
        match self {
            InjectionStatus::Idle => "Ready.".to_string(),
            InjectionStatus::Scanning => "Scanning for Minecraft…".to_string(),
            InjectionStatus::Injecting(pid) => format!("Injecting into process {pid}…"),
            InjectionStatus::Done { pid, version } => {
                format!("Injected into process {pid} (Minecraft {version}).")
            }
            InjectionStatus::Failed(reason) => format!("Error: {reason}"),
        }
    }
}

/// Front-end-independent injector state machine.
pub struct InjectorApp {
    processes: Vec<ProcessInfo>,
    selected_pid: Option<u32>,
    status: InjectionStatus,
    /// Inject even into an unsupported Minecraft version — a development
    /// override.
    force: bool,
    /// Result channel of an in-flight injection, with its target pid.
    pending: Option<(u32, Receiver<Result<String, InjectError>>)>,
}

impl InjectorApp {
    /// Creates an idle app with no scan results.
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            selected_pid: None,
            status: InjectionStatus::Idle,
            force: false,
            pending: None,
        }
    }

    /// Whether the version check is overridden.
    pub fn force(&self) -> bool {
        self.force
    }

    /// Overrides — or restores — the version check for later injections.
    pub fn set_force(&mut self, force: bool) {
        self.force = force;
    }

    /// The processes found by the last [`scan`](Self::scan).
    pub fn processes(&self) -> &[ProcessInfo] {
        &self.processes
    }

    /// The currently selected process id, if any.
    pub fn selected_pid(&self) -> Option<u32> {
        self.selected_pid
    }

    /// The current status.
    pub fn status(&self) -> &InjectionStatus {
        &self.status
    }

    /// Whether an injection is currently running.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Selects a process by pid, if it is in the current list.
    pub fn select(&mut self, pid: u32) {
        if self.processes.iter().any(|p| p.pid == pid) {
            self.selected_pid = Some(pid);
        }
    }

    /// Rescans for Minecraft processes. Keeps the current selection if it is
    /// still present, otherwise selects the first result.
    pub fn scan(&mut self) {
        if self.is_busy() {
            return;
        }
        self.status = InjectionStatus::Scanning;
        self.processes = find_minecraft_processes();

        let kept = self
            .selected_pid
            .is_some_and(|pid| self.processes.iter().any(|p| p.pid == pid));
        if !kept {
            self.selected_pid = self.processes.first().map(|p| p.pid);
        }
        self.status = InjectionStatus::Idle;
    }

    /// Starts injecting into the selected process on a worker thread. A no-op
    /// when nothing is selected or an injection is already running.
    pub fn start_injection(&mut self) {
        if self.is_busy() {
            return;
        }
        let Some(pid) = self.selected_pid else {
            return;
        };

        let version = self
            .processes
            .iter()
            .find(|p| p.pid == pid)
            .and_then(|p| p.version.clone());
        let force = self.force;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(inject::inject(pid, version.as_deref(), force));
        });
        self.pending = Some((pid, rx));
        self.status = InjectionStatus::Injecting(pid);
    }

    /// Polls the injection worker. Front-ends call this once per frame/loop;
    /// returns `true` when the status changed so the GUI knows to repaint.
    pub fn poll(&mut self) -> bool {
        let Some((pid, rx)) = &self.pending else {
            return false;
        };
        let pid = *pid;
        match rx.try_recv() {
            Ok(result) => {
                self.status = match result {
                    Ok(version) => InjectionStatus::Done { pid, version },
                    Err(e) => InjectionStatus::Failed(e.to_string()),
                };
                self.pending = None;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.status =
                    InjectionStatus::Failed("injection worker stopped unexpectedly".to_string());
                self.pending = None;
                true
            }
        }
    }
}

impl Default for InjectorApp {
    fn default() -> Self {
        Self::new()
    }
}
