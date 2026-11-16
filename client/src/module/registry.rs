//! The module registry — every registered module, keyed by [`ModuleId`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use log::{error, info};

use crate::module::{KeyboardKey, Module, ModuleId, ModuleSetting, ModuleType};
use crate::net::packet::{Packet, PacketAction};

/// A shared, lockable handle to one module.
pub type ModuleHandle = Arc<Mutex<ModuleType>>;

/// A module's factory defaults — the keybind and settings it was registered
/// with. Kept so "Reset Settings" can restore them.
struct ModuleDefaults {
    key_bind: KeyboardKey,
    settings: Vec<ModuleSetting>,
}

/// Holds every registered module. Backed by a `DashMap` so the render thread
/// can read it each frame without contending on a single global lock.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: DashMap<ModuleId, ModuleHandle>,
    /// Factory defaults captured at registration, keyed like `modules`.
    defaults: DashMap<ModuleId, ModuleDefaults>,
    /// Whether modules may currently run — true only in a world with a living
    /// player. At the main menu, or while the player is dead on the respawn
    /// screen, modules stay armed (`enabled`) but inactive.
    active: AtomicBool,
    /// The last tick error of each failing module, so it is logged once
    /// rather than every tick.
    failing: DashMap<ModuleId, String>,
}

/// Runs `on_start` or `on_stop`, logging a failure under the module's name.
fn start_or_stop(module: &dyn Module, start: bool) {
    let (phase, result) = if start {
        ("on_start", module.on_start())
    } else {
        ("on_stop", module.on_stop())
    };
    if let Err(e) = result {
        error!("{}: {phase} failed: {e}", module.get_module_data().name());
    }
}

impl ModuleRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a module under its [`ModuleId`], capturing its factory
    /// defaults so they can be restored later.
    pub fn register<M>(&self, module: M)
    where
        M: Module + Send + Sync + 'static,
    {
        let module: ModuleType = Box::new(module);
        let data = module.get_module_data();
        let id = data.id;
        self.defaults.insert(
            id,
            ModuleDefaults {
                key_bind: data.key_bind,
                settings: data.settings.clone(),
            },
        );
        self.modules.insert(id, Arc::new(Mutex::new(module)));
    }

    /// Restores every module to its factory defaults: default keybind, default
    /// setting values, and disabled. Backs the GUI's "Reset Settings" button.
    pub fn reset_settings(&self) {
        for entry in self.modules.iter() {
            let Some(defaults) = self.defaults.get(entry.key()) else {
                continue;
            };
            let Ok(mut module) = entry.value().lock() else {
                continue;
            };
            if module.get_module_data().enabled {
                self.set_enabled(&mut module, false);
            }
            let data = module.get_module_data_mut();
            data.key_bind = defaults.key_bind;
            data.settings = defaults.settings.clone();
        }
    }

    /// Arms or disarms a module. It starts or stops right away only while
    /// modules are active; otherwise on the next transition.
    pub fn set_enabled(&self, module: &mut ModuleType, enabled: bool) {
        module.get_module_data_mut().set_enabled(enabled);
        if self.is_active() {
            start_or_stop(module.as_ref(), enabled);
        }
    }

    /// Whether modules may currently run — in a world, with a living player.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Updates whether modules may run. On every transition each *enabled*
    /// module is started or stopped — so a module armed at the menu only truly
    /// runs once the player is in-game and alive, and stands down again when
    /// they leave the world or die.
    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, Ordering::SeqCst) == active {
            return; // no transition
        }
        if active {
            info!("Modules active: in a world with a living player");
        } else {
            info!("Modules inactive: no world, or the player is dead");
        }
        for handle in self.handles() {
            let Ok(module) = handle.lock() else {
                continue;
            };
            if module.get_module_data().enabled {
                start_or_stop(module.as_ref(), active);
            }
        }
    }

    /// A handle to one module by id.
    pub fn get(&self, id: ModuleId) -> Option<ModuleHandle> {
        self.modules.get(&id).map(|entry| Arc::clone(entry.value()))
    }

    /// Handles to every module. Snapshotted, so the caller holds no shard
    /// locks while it works with the modules.
    pub fn handles(&self) -> Vec<ModuleHandle> {
        self.modules
            .iter()
            .map(|entry| Arc::clone(entry.value()))
            .collect()
    }

    /// Every module keyed by id — an owned snapshot.
    pub fn by_id(&self) -> HashMap<ModuleId, ModuleHandle> {
        self.modules
            .iter()
            .map(|entry| (*entry.key(), Arc::clone(entry.value())))
            .collect()
    }

    /// Ticks every enabled module once. A module whose tick fails is stopped
    /// rather than aborting the whole pass.
    pub fn tick(&self) {
        for handle in self.handles() {
            let Ok(module) = handle.lock() else {
                continue;
            };
            let data = module.get_module_data();
            if !data.enabled {
                continue;
            }
            let Err(e) = module.on_tick() else {
                self.failing.remove(&data.id);
                continue;
            };
            let message = e.to_string();
            let repeated = self
                .failing
                .get(&data.id)
                .is_some_and(|last| *last == message);
            if !repeated {
                error!("{}: on_tick failed, stopping it: {message}", data.name());
                self.failing.insert(data.id, message);
            }
            if let Err(e) = module.on_stop() {
                if !repeated {
                    error!("{}: on_stop failed: {e}", data.name());
                }
            }
        }
    }

    /// Offers `packet` to every enabled module's `handle_packet`. Called from
    /// the connection's packet dispatch, on the Netty thread. Returns
    /// [`PacketAction::Cancel`] as soon as any module asks to drop the packet
    /// (the remaining modules are then skipped), otherwise
    /// [`PacketAction::Forward`].
    pub fn handle_packet(&self, packet: &mut Packet) -> PacketAction {
        if !self.is_active() {
            return PacketAction::Forward;
        }
        for handle in self.handles() {
            let Ok(module) = handle.lock() else {
                continue;
            };
            if !module.get_module_data().enabled {
                continue;
            }
            if module.handle_packet(packet) == PacketAction::Cancel {
                return PacketAction::Cancel;
            }
        }
        PacketAction::Forward
    }
}
