use mc_bindings::{GameRenderer, Result};

/// Inputs of `GameRenderer.bobHurt` / `bobView` for the last frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraBob {
    pub view_bobbing: bool,
    pub damage_tilt_strength: f32,
    pub is_player: bool,
    pub is_living: bool,
    pub is_dead_or_dying: bool,
    pub death_time: f32,
    pub hurt_time: f32,
    pub hurt_duration: f32,
    pub hurt_dir: f32,
    pub walk_distance: f32,
    pub bob: f32,
}

pub fn camera_bob(renderer: &GameRenderer) -> Result<CameraBob> {
    let game = renderer.game_render_state()?;
    let options = game.options_render_state()?;
    let entity = game
        .level_render_state()?
        .camera_render_state()?
        .entity_render_state()?;
    Ok(CameraBob {
        view_bobbing: options.bob_view()?,
        damage_tilt_strength: options.damage_tilt_strength()? as f32,
        is_player: entity.is_player()?,
        is_living: entity.is_living()?,
        is_dead_or_dying: entity.is_dead_or_dying()?,
        death_time: entity.death_time()?,
        hurt_time: entity.hurt_time()?,
        hurt_duration: entity.hurt_duration()? as f32,
        hurt_dir: entity.hurt_dir()?,
        walk_distance: entity.backwards_interpolated_walk_distance()?,
        bob: entity.bob()?,
    })
}
