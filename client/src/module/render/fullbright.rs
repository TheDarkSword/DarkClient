use crate::game::prelude::*;
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId};
use std::sync::Mutex;

/// Brightness forced onto the gamma option while Fullbright is active — far
/// above the `1.0` slider maximum, enough to light caves fully.
const FULLBRIGHT_GAMMA: f64 = 16.0;
/// Vanilla default gamma — the fallback used when the player's own value
/// cannot be captured.
const DEFAULT_GAMMA: f64 = 0.5;

/// Forces the game brightness to maximum, lighting up caves and the night.
///
/// Works by writing the gamma `OptionInstance` past its normal `0..1` slider
/// range (see [`OptionInstance::force_double`]); the original value is captured
/// on start and restored on stop.
#[derive(Debug)]
pub struct FullbrightModule {
    pub module: ModuleData,
    /// The gamma value to restore when Fullbright is turned off — captured by
    /// [`on_start`](Module::on_start).
    original_gamma: Mutex<Option<f64>>,
}

impl FullbrightModule {
    pub fn new() -> Self {
        Self {
            module: ModuleData {
                id: ModuleId::Fullbright,
                description: "Lights up the world to maximum brightness".to_string(),
                category: ModuleCategory::Render,
                key_bind: KeyboardKey::KeyNone,
                enabled: false,
                settings: vec![],
            },
            original_gamma: Mutex::new(None),
        }
    }
}

impl Module for FullbrightModule {
    fn on_start(&self) -> anyhow::Result<()> {
        let gamma = minecraft().options()?.gamma()?;
        let current = gamma.get_double().unwrap_or(DEFAULT_GAMMA);
        // A value above the slider range means Fullbright is already applied
        // (e.g. after a hot-reload) — never capture our own injected value as
        // the original; fall back to the vanilla default instead.
        let original = if current.is_finite() && (0.0..=1.0).contains(&current) {
            current
        } else {
            DEFAULT_GAMMA
        };
        *self.original_gamma.lock().unwrap() = Some(original);
        gamma.force_double(FULLBRIGHT_GAMMA)
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        let original = self
            .original_gamma
            .lock()
            .unwrap()
            .take()
            .unwrap_or(DEFAULT_GAMMA);
        minecraft().options()?.gamma()?.force_double(original)
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        // Re-assert each tick so opening the video-settings slider — which
        // would clamp gamma back into `0..1` — cannot quietly undo Fullbright.
        minecraft()
            .options()?
            .gamma()?
            .force_double(FULLBRIGHT_GAMMA)
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
