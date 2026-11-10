use crate::game::prelude::*;
use crate::module::combat::criticals::{self, CritState};
use crate::module::combat::{look_at, pick_target};
use crate::module::{KeyboardKey, Module, ModuleCategory, ModuleData, ModuleId, ModuleSetting};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Which entities an aura attacks.
#[derive(Debug, Clone, Copy)]
pub enum AuraTarget {
    Players,
    Mobs,
}

/// Cross-tick combat state.
#[derive(Debug, Default)]
struct AuraState {
    /// Network id of the locked target, if any.
    target: Option<i32>,
    /// When the last hit landed — drives the CPS limiter.
    last_attack: Option<Instant>,
}

#[derive(Debug)]
pub struct BaseAura {
    pub module: ModuleData,
    pub target: AuraTarget,
    state: Mutex<AuraState>,
}

impl BaseAura {
    pub fn new(
        id: ModuleId,
        description: String,
        key_bind: KeyboardKey,
        target: AuraTarget,
    ) -> Self {
        Self {
            module: ModuleData {
                id,
                description,
                category: ModuleCategory::Combat,
                key_bind,
                enabled: false,
                settings: vec![
                    ModuleSetting::Slider {
                        name: "Range".to_string(),
                        value: 3.5,
                        min: 3.0,
                        max: 6.0,
                    },
                    ModuleSetting::Slider {
                        name: "Speed".to_string(),
                        value: 7.0,
                        min: 2.0,
                        max: 20.0,
                    },
                    ModuleSetting::Slider {
                        name: "Attack Angle".to_string(),
                        value: 50.0,
                        min: 10.0,
                        max: 180.0,
                    },
                    ModuleSetting::Slider {
                        name: "CPS".to_string(),
                        value: 12.0,
                        min: 1.0,
                        max: 20.0,
                    },
                    ModuleSetting::Toggle {
                        name: "Cooldown".to_string(),
                        value: true,
                    },
                ],
            },
            target,
            state: Mutex::new(AuraState::default()),
        }
    }

    fn slider(&self, name: &str, fallback: f32) -> f32 {
        self.module
            .get_setting(name)
            .and_then(|setting| setting.get_slider_value())
            .unwrap_or(fallback)
    }

    pub fn get_range(&self) -> f32 {
        self.slider("Range", 3.5)
    }

    fn speed(&self) -> f32 {
        self.slider("Speed", 7.0)
    }

    fn attack_angle(&self) -> f32 {
        self.slider("Attack Angle", 50.0)
    }

    fn cps(&self) -> f32 {
        self.slider("CPS", 12.0)
    }

    fn respect_cooldown(&self) -> bool {
        self.module
            .get_setting("Cooldown")
            .and_then(|setting| setting.get_toggle_value())
            .unwrap_or(true)
    }

    /// Whether `entity` is the kind of entity this aura attacks.
    fn is_target(&self, entity: &Entity) -> bool {
        match self.target {
            AuraTarget::Players => entity.instance_of::<Player>(),
            AuraTarget::Mobs => entity.instance_of::<Mob>(),
        }
    }

    /// Whether a new hit is allowed now — the CPS limiter, plus the optional
    /// 1.9+ attack-cooldown check.
    fn can_attack(&self, player: &LocalPlayer) -> anyhow::Result<bool> {
        let interval = Duration::from_secs_f32(1.0 / self.cps());
        let cps_ready = self
            .state
            .lock()
            .unwrap()
            .last_attack
            .is_none_or(|last| last.elapsed() >= interval);
        if !cps_ready {
            return Ok(false);
        }
        if self.respect_cooldown() && player.get_attack_strength_scale(0.5)? < 1.0 {
            return Ok(false);
        }
        Ok(true)
    }
}

impl Module for BaseAura {
    fn on_start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_stop(&self) -> anyhow::Result<()> {
        *self.state.lock().unwrap() = AuraState::default();
        Ok(())
    }

    fn on_tick(&self) -> anyhow::Result<()> {
        let minecraft = minecraft();
        // Stand down while a menu (inventory, chest, crafting, chat, …) is open.
        if minecraft.current_screen().is_open() {
            return Ok(());
        }
        let (Some(player), Some(world), Some(game_mode)) = (
            minecraft.player()?,
            minecraft.level()?,
            minecraft.game_mode()?,
        ) else {
            *self.state.lock().unwrap() = AuraState::default();
            return Ok(()); // not in a world — nothing to do
        };

        let entities = world.entities_for_rendering()?;
        let eye = player.get_eye_position()?;
        let self_id = player.get_id()?;
        let range = self.get_range() as f64;

        let locked = self.state.lock().unwrap().target;
        let Some((target_id, target)) =
            pick_target(&entities, eye, range * range, self_id, locked, |entity| {
                self.is_target(entity)
            })
        else {
            self.state.lock().unwrap().target = None;
            return Ok(());
        };
        self.state.lock().unwrap().target = Some(target_id);

        // Aim smoothly toward the target (the rotation controller eases the
        // camera there frame by frame).
        let angle = look_at(&player, &target, self.speed(), 180.0)?;

        // Attack once roughly aligned and the timers allow it. With Criticals
        // armed the hit is held back until the player is hopped into the crit
        // window; the CPS/cooldown timers only advance on a hit that lands.
        if angle <= self.attack_angle()
            && self.can_attack(&player)?
            && criticals::prepare(&player)? == CritState::Ready
        {
            game_mode.attack(&player, &target)?;
            player.swing(InteractionHand::MainHand)?;
            self.state.lock().unwrap().last_attack = Some(Instant::now());
        }

        Ok(())
    }

    fn get_module_data(&self) -> &ModuleData {
        &self.module
    }

    fn get_module_data_mut(&mut self) -> &mut ModuleData {
        &mut self.module
    }
}
