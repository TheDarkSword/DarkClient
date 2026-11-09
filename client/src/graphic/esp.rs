//! ESP overlay: 3D wireframe boxes for players, mobs and containers.
//!
//! # Performance model
//!
//! The expensive part of an ESP is the JNI traffic — reading every entity's
//! position, type and stats. Doing that per frame is what tanks the FPS, so it
//! is **decoupled from the frame rate**:
//!
//! * [`gather`] (all the JNI work) runs once per game tick.
//! * Every frame only reads the camera, the partial tick and the view-bobbing
//!   state (a few JNI calls) and does pure-CPU projection + egui drawing.
//! * Each entity is drawn at `lerp(partial_tick, old, current)` — exactly
//!   where Minecraft renders it — and the projection replays Minecraft's view
//!   bobbing and damage tilt, so boxes stay on the model while the entity or
//!   the player moves.
//! * Every JNI scope is wrapped in a local-reference frame: without this the
//!   JVM local-ref table grows unbounded, which is itself a slow FPS killer.
//!
//! The chest scan is heavier (it walks loaded chunks) so it runs even rarer,
//! every [`CHEST_SCAN_INTERVAL`].

use crate::mapping::client::camera::Camera;
use crate::mapping::client::delta_tracker::DeltaTracker;
use crate::mapping::client::game_renderer::{CameraBob, GameRenderer};
use crate::mapping::client::world::World;
use crate::mapping::entity::mob::Mob;
use crate::mapping::entity::player::Player;
use crate::mapping::entity::{Entity, EntityRef, LivingEntityRef};
use crate::mapping::math::Vec3;
use crate::mapping::MappedObject;
use crate::module::{ModuleId, ModuleSetting};
use crate::state::{client, minecraft};
use egui::{
    pos2, vec2, Align2, Color32, Context, FontId, Id, LayerId, Order, Painter, Pos2, Rect,
    Rounding, Stroke,
};
use std::f64::consts::PI;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Wall-time between two chest scans — chests do not move, so this is rare.
const CHEST_SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Half-extent, in chunks, of the area scanned for containers.
const CHEST_CHUNK_RADIUS: i32 = 8;
/// Box edge thickness, in points.
const LINE_WIDTH: f32 = 1.6;

// --- math ------------------------------------------------------------------

/// A plain 3D vector — kept separate from egui/JNI types so the projection
/// math stays allocation- and dependency-free.
#[derive(Debug, Clone, Copy)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}

impl V3 {
    fn sub(self, o: V3) -> V3 {
        V3 {
            x: self.x - o.x,
            y: self.y - o.y,
            z: self.z - o.z,
        }
    }

    fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    fn cross(self, o: V3) -> V3 {
        V3 {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    fn lerp(self, o: V3, t: f64) -> V3 {
        V3 {
            x: self.x + (o.x - self.x) * t,
            y: self.y + (o.y - self.y) * t,
            z: self.z + (o.z - self.z) * t,
        }
    }

    fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
}

/// Converts a JNI-snapshot [`Vec3`] into the projection-math vector.
impl From<Vec3> for V3 {
    fn from(v: Vec3) -> V3 {
        V3 {
            x: v.x(),
            y: v.y(),
            z: v.z(),
        }
    }
}

/// An affine transform of camera space (`+x` right, `+y` up, looking down
/// `-z`) — how Minecraft's view bobbing and damage tilt move the world.
#[derive(Debug, Clone, Copy)]
struct Affine {
    m: [[f64; 3]; 3],
    t: [f64; 3],
}

impl Affine {
    const IDENTITY: Affine = Affine {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    fn translation(x: f64, y: f64, z: f64) -> Affine {
        Affine {
            t: [x, y, z],
            ..Affine::IDENTITY
        }
    }

    fn rotation_x(degrees: f64) -> Affine {
        let (s, c) = degrees.to_radians().sin_cos();
        Affine {
            m: [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
            t: [0.0; 3],
        }
    }

    fn rotation_y(degrees: f64) -> Affine {
        let (s, c) = degrees.to_radians().sin_cos();
        Affine {
            m: [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
            t: [0.0; 3],
        }
    }

    fn rotation_z(degrees: f64) -> Affine {
        let (s, c) = degrees.to_radians().sin_cos();
        Affine {
            m: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    /// The product `self · other`: `other` applies first, the order
    /// `PoseStack.mulPose` / `translate` compose in.
    fn mul(self, other: Affine) -> Affine {
        let mut m = [[0.0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.m[i][k] * other.m[k][j]).sum();
            }
        }
        Affine {
            m,
            t: self.apply(other.t),
        }
    }

    fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let mut out = self.t;
        for (i, o) in out.iter_mut().enumerate() {
            *o += (0..3).map(|k| self.m[i][k] * v[k]).sum::<f64>();
        }
        out
    }
}

/// Replays `GameRenderer.bobHurt` then `bobView`, which Minecraft multiplies
/// into its projection matrix.
fn bob_transform(bob: &CameraBob) -> Affine {
    let mut pose = Affine::IDENTITY;
    if bob.is_living {
        if bob.is_dead_or_dying {
            let duration = bob.death_time.min(20.0) as f64;
            pose = pose.mul(Affine::rotation_z(40.0 - 8000.0 / (duration + 200.0)));
        }
        if bob.hurt_time >= 0.0 && bob.hurt_duration > 0.0 {
            let hurt = (bob.hurt_time / bob.hurt_duration) as f64;
            let hurt = (hurt.powi(4) * PI).sin();
            let dir = bob.hurt_dir as f64;
            let tilt = -hurt * 14.0 * bob.damage_tilt_strength as f64;
            pose = pose
                .mul(Affine::rotation_y(-dir))
                .mul(Affine::rotation_z(tilt))
                .mul(Affine::rotation_y(dir));
        }
    }
    if bob.view_bobbing && bob.is_player {
        let walk = bob.walk_distance as f64 * PI;
        let amount = bob.bob as f64;
        pose = pose
            .mul(Affine::translation(
                walk.sin() * amount * 0.5,
                -(walk.cos() * amount).abs(),
                0.0,
            ))
            .mul(Affine::rotation_z(walk.sin() * amount * 3.0))
            .mul(Affine::rotation_x(
                ((walk - 0.2).cos() * amount).abs() * 5.0,
            ));
    }
    pose
}

/// A camera, reduced to exactly what world→screen projection needs.
struct View {
    cam: V3,
    fwd: V3,
    right: V3,
    up: V3,
    tan_x: f64,
    tan_y: f64,
    w: f32,
    h: f32,
    bob: Affine,
}

impl View {
    /// Projects a world point to logical screen coordinates, or `None` if it
    /// lies behind the camera (where a perspective divide is meaningless).
    fn project(&self, p: V3) -> Option<Pos2> {
        let r = p.sub(self.cam);
        // Camera space as Minecraft's view matrix produces it, then the
        // bobbing Minecraft folds into its projection.
        let [x, y, z] = self
            .bob
            .apply([r.dot(self.right), r.dot(self.up), -r.dot(self.fwd)]);
        let depth = -z;
        if depth < 0.05 {
            return None;
        }
        let ndc_x = (x / depth) / self.tan_x;
        let ndc_y = (y / depth) / self.tan_y;
        let sx = (ndc_x * 0.5 + 0.5) * self.w as f64;
        let sy = (0.5 - ndc_y * 0.5) * self.h as f64;
        Some(pos2(sx as f32, sy as f32))
    }
}

/// Builds a [`View`] from Minecraft's camera state.
fn build_view(
    cam: V3,
    yaw_deg: f32,
    pitch_deg: f32,
    fov_deg: f64,
    w: f32,
    h: f32,
    bob: Affine,
) -> View {
    let yaw = (yaw_deg as f64).to_radians();
    let pitch = (pitch_deg as f64).to_radians();
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();

    // Minecraft yaw: 0° faces +Z, increasing clockwise. Pitch: positive looks
    // down. The forward vector follows directly from those conventions.
    let fwd = V3 {
        x: -sy * cp,
        y: -sp,
        z: cy * cp,
    };
    let right = V3 {
        x: -cy,
        y: 0.0,
        z: -sy,
    };
    let up = right.cross(fwd);

    let tan_y = (fov_deg.to_radians() * 0.5).tan();
    let tan_x = tan_y * (w / h) as f64;

    View {
        cam,
        fwd,
        right,
        up,
        tan_x,
        tan_y,
        w,
        h,
        bob,
    }
}

// --- gathered snapshot -----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Player,
    Mob,
}

/// One living target, as captured by the last [`gather`].
#[derive(Debug, Clone)]
struct EntityTarget {
    kind: TargetKind,
    /// Position at the previous tick (`xOld` / `yOld` / `zOld`).
    old: V3,
    /// Position at the latest tick.
    pos: V3,
    width: f64,
    height: f64,
    name: String,
    health: f32,
    max_health: f32,
}

/// One container block, by its lower-corner block coordinates.
#[derive(Debug, Clone, Copy)]
struct ChestTarget {
    pos: V3,
}

/// Cross-frame ESP state: the cached camera handle and the latest snapshot.
struct EspState {
    renderer: Option<GameRenderer>,
    camera: Option<Camera>,
    delta_tracker: Option<DeltaTracker>,
    entities: Vec<EntityTarget>,
    chests: Vec<ChestTarget>,
    /// Game tick the entity snapshot was taken at.
    gathered_tick: Option<i32>,
    last_chest_scan: Option<Instant>,
    /// Last field of view read from the camera, reused if a read fails.
    fov: f64,
    /// Set once camera resolution has failed, so the failure is logged once.
    camera_logged: bool,
    /// Set once the bobbing state could not be read, so the projection stays
    /// unbobbed without retrying every frame.
    bob_unavailable: bool,
    /// Set once the partial tick could not be read, likewise.
    partial_tick_unavailable: bool,
}

impl EspState {
    fn new() -> Self {
        EspState {
            renderer: None,
            camera: None,
            delta_tracker: None,
            entities: Vec::new(),
            chests: Vec::new(),
            gathered_tick: None,
            last_chest_scan: None,
            fov: 70.0,
            camera_logged: false,
            bob_unavailable: false,
            partial_tick_unavailable: false,
        }
    }
}

fn state() -> &'static Mutex<EspState> {
    static STATE: OnceLock<Mutex<EspState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(EspState::new()))
}

// --- module configuration --------------------------------------------------

/// Per-category drawing options, snapshotted from the modules once per frame.
struct EntityCfg {
    enabled: bool,
    color: Color32,
    show_name: bool,
    show_distance: bool,
    show_health: bool,
    /// Maximum distance, in blocks, an entity is drawn (and processed) at.
    range: f32,
}

struct ChestCfg {
    enabled: bool,
    color: Color32,
    show_distance: bool,
}

/// Tracer-line options, snapshotted from the Tracers module once per frame.
struct TracerCfg {
    enabled: bool,
    color: Color32,
    players: bool,
    mobs: bool,
    /// Maximum distance, in blocks, a tracer is drawn at.
    range: f32,
}

/// Name-tag options, snapshotted from the Nametags module once per frame.
struct NametagCfg {
    enabled: bool,
    color: Color32,
    show_health: bool,
    show_distance: bool,
    /// Maximum distance, in blocks, a name tag is drawn at.
    range: f32,
}

struct EspConfig {
    player: EntityCfg,
    mob: EntityCfg,
    chest: ChestCfg,
    tracer: TracerCfg,
    nametag: NametagCfg,
}

impl EspConfig {
    fn any_enabled(&self) -> bool {
        self.player.enabled
            || self.mob.enabled
            || self.chest.enabled
            || self.tracer.enabled
            || self.nametag.enabled
    }

    /// Whether players must be gathered — wanted by Player ESP, by tracers, or
    /// by name tags.
    fn want_players(&self) -> bool {
        self.player.enabled || (self.tracer.enabled && self.tracer.players) || self.nametag.enabled
    }

    /// Whether mobs must be gathered — wanted by Mob ESP or by tracers.
    fn want_mobs(&self) -> bool {
        self.mob.enabled || (self.tracer.enabled && self.tracer.mobs)
    }
}

fn color_setting(data: &crate::module::ModuleData, name: &str, fallback: Color32) -> Color32 {
    match data.get_setting(name) {
        Some(ModuleSetting::Color { value, .. }) => Color32::from_rgba_unmultiplied(
            (value[0] * 255.0) as u8,
            (value[1] * 255.0) as u8,
            (value[2] * 255.0) as u8,
            (value[3] * 255.0).max(40.0) as u8,
        ),
        _ => fallback,
    }
}

fn toggle_setting(data: &crate::module::ModuleData, name: &str, fallback: bool) -> bool {
    data.get_setting(name)
        .and_then(ModuleSetting::get_toggle_value)
        .unwrap_or(fallback)
}

fn slider_setting(data: &crate::module::ModuleData, name: &str, fallback: f32) -> f32 {
    data.get_setting(name)
        .and_then(ModuleSetting::get_slider_value)
        .unwrap_or(fallback)
}

/// Reads the three ESP modules' state with a single lock pass.
fn read_config() -> EspConfig {
    let disabled_entity = || EntityCfg {
        enabled: false,
        color: Color32::WHITE,
        show_name: false,
        show_distance: false,
        show_health: false,
        range: 0.0,
    };
    let mut cfg = EspConfig {
        player: disabled_entity(),
        mob: disabled_entity(),
        chest: ChestCfg {
            enabled: false,
            color: Color32::WHITE,
            show_distance: false,
        },
        tracer: TracerCfg {
            enabled: false,
            color: Color32::WHITE,
            players: false,
            mobs: false,
            range: 0.0,
        },
        nametag: NametagCfg {
            enabled: false,
            color: Color32::WHITE,
            show_health: false,
            show_distance: false,
            range: 0.0,
        },
    };

    let modules = &client().modules;

    if let Some(arc) = modules.get(ModuleId::PlayerEsp) {
        if let Ok(module) = arc.lock() {
            let data = module.get_module_data();
            cfg.player = EntityCfg {
                enabled: data.enabled,
                color: color_setting(data, "Color", Color32::from_rgb(255, 70, 70)),
                show_name: toggle_setting(data, "Name", true),
                show_distance: toggle_setting(data, "Distance", true),
                show_health: toggle_setting(data, "Health", true),
                range: slider_setting(data, "Range", 64.0),
            };
        }
    }
    if let Some(arc) = modules.get(ModuleId::MobEsp) {
        if let Ok(module) = arc.lock() {
            let data = module.get_module_data();
            cfg.mob = EntityCfg {
                enabled: data.enabled,
                color: color_setting(data, "Color", Color32::from_rgb(255, 215, 50)),
                show_name: toggle_setting(data, "Name", true),
                show_distance: toggle_setting(data, "Distance", true),
                show_health: toggle_setting(data, "Health", true),
                range: slider_setting(data, "Range", 64.0),
            };
        }
    }
    if let Some(arc) = modules.get(ModuleId::ChestEsp) {
        if let Ok(module) = arc.lock() {
            let data = module.get_module_data();
            cfg.chest = ChestCfg {
                enabled: data.enabled,
                color: color_setting(data, "Color", Color32::from_rgb(255, 140, 30)),
                show_distance: toggle_setting(data, "Distance", true),
            };
        }
    }
    if let Some(arc) = modules.get(ModuleId::Tracers) {
        if let Ok(module) = arc.lock() {
            let data = module.get_module_data();
            cfg.tracer = TracerCfg {
                enabled: data.enabled,
                color: color_setting(data, "Color", Color32::from_rgb(100, 200, 255)),
                players: toggle_setting(data, "Players", true),
                mobs: toggle_setting(data, "Mobs", false),
                range: slider_setting(data, "Range", 96.0),
            };
        }
    }
    if let Some(arc) = modules.get(ModuleId::Nametags) {
        if let Ok(module) = arc.lock() {
            let data = module.get_module_data();
            cfg.nametag = NametagCfg {
                enabled: data.enabled,
                color: color_setting(data, "Color", Color32::WHITE),
                show_health: toggle_setting(data, "Health", true),
                show_distance: toggle_setting(data, "Distance", false),
                range: slider_setting(data, "Range", 96.0),
            };
        }
    }

    cfg
}

// --- public entry point ----------------------------------------------------

/// Draws the whole ESP overlay for the current frame.
///
/// Called once per frame from the overlay renderer; cheap when every ESP
/// module is disabled.
pub fn draw(ctx: &Context) {
    let cfg = read_config();
    let mut state = state().lock().unwrap();

    // Nothing to show at the menu or while dead — and the snapshot must not
    // linger into the next world.
    if !cfg.any_enabled() || !client().modules.is_active() {
        state.entities.clear();
        state.chests.clear();
        state.gathered_tick = None;
        return;
    }

    let tick = crate::graphic::hook::last_tick();
    if state.gathered_tick != Some(tick) {
        state.gathered_tick = Some(tick);
        gather(&mut state, &cfg, Instant::now());
    }

    let view = match read_view(&mut state, ctx) {
        Some(view) => view,
        None => return,
    };
    let t = read_partial_tick(&mut state);

    let painter = ctx.layer_painter(LayerId::new(Order::Background, Id::new("esp_overlay")));

    for entity in &state.entities {
        draw_entity(&painter, &view, entity, t, &cfg);
        if cfg.tracer.enabled {
            draw_tracer(&painter, &view, entity, t, &cfg.tracer);
        }
        if cfg.nametag.enabled {
            draw_nametag(&painter, &view, entity, t, &cfg.nametag);
        }
    }
    for chest in &state.chests {
        draw_chest(&painter, &view, chest, &cfg);
    }
}

/// How far into the current tick this frame is — the factor Minecraft
/// interpolated entities with. `1.0` (the latest positions) if unreadable.
fn read_partial_tick(state: &mut EspState) -> f64 {
    if state.partial_tick_unavailable {
        return 1.0;
    }
    if state.delta_tracker.is_none() {
        state.delta_tracker = minecraft().delta_tracker().ok();
    }
    let read = state
        .delta_tracker
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no DeltaTracker"))
        .and_then(DeltaTracker::partial_tick);
    match read {
        Ok(t) if t.is_finite() => (t as f64).clamp(0.0, 1.0),
        Ok(_) => 1.0,
        Err(e) => {
            state.partial_tick_unavailable = true;
            log::warn!("ESP: partial tick unavailable, drawing latest positions: {e}");
            1.0
        }
    }
}

/// The view-bobbing / damage-tilt transform of the frame just rendered.
fn read_bob(state: &mut EspState) -> Affine {
    if state.bob_unavailable {
        return Affine::IDENTITY;
    }
    let Some(renderer) = state.renderer.as_ref() else {
        return Affine::IDENTITY;
    };
    match renderer.camera_bob() {
        Ok(bob) => bob_transform(&bob),
        Err(e) => {
            state.bob_unavailable = true;
            log::info!("ESP: view bobbing not compensated on this version: {e}");
            Affine::IDENTITY
        }
    }
}

// --- camera ----------------------------------------------------------------

/// Resolves the current camera into a [`View`], caching the [`Camera`] handle.
fn read_view(state: &mut EspState, ctx: &Context) -> Option<View> {
    if state.camera.is_none() {
        let resolved = minecraft()
            .game_renderer()
            .and_then(|renderer| Ok((renderer.get_main_camera()?, renderer)));
        match resolved {
            Ok((camera, renderer)) => {
                state.camera = Some(camera);
                state.renderer = Some(renderer);
            }
            Err(e) => {
                if !state.camera_logged {
                    state.camera_logged = true;
                    log::warn!("ESP: camera unavailable: {e}");
                }
                return None;
            }
        }
    }

    let camera = state.camera.clone()?;
    let rect = ctx.screen_rect();
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return None;
    }

    let read = (|| -> anyhow::Result<(V3, f32, f32)> {
        Ok((camera.position()?.into(), camera.yaw()?, camera.pitch()?))
    })();

    let (cam_pos, yaw, pitch) = match read {
        Ok(values) => values,
        Err(error) => {
            if !state.camera_logged {
                state.camera_logged = true;
                log::warn!("ESP: camera read failed: {error}");
            }
            return None;
        }
    };

    let fov_deg = resolve_fov(&camera, state);
    let bob = read_bob(state);

    Some(build_view(
        cam_pos,
        yaw,
        pitch,
        fov_deg,
        rect.width(),
        rect.height(),
        bob,
    ))
}

/// The vertical FOV the frame was rendered with. `Camera.fov` is rewritten
/// every frame with the player modifier, the `fovEffectScale` blend and the
/// water / lava / dying tweaks already folded in.
fn resolve_fov(camera: &Camera, state: &mut EspState) -> f64 {
    if let Ok(fov) = camera.fov() {
        if fov.is_finite() && fov > 0.0 {
            state.fov = (fov as f64).clamp(1.0, 179.0);
        }
    }
    state.fov
}

// --- gather ----------------------------------------------------------------

/// Refreshes the snapshot: entities every call, chests on their own schedule.
fn gather(state: &mut EspState, cfg: &EspConfig, now: Instant) {
    let want_players = cfg.want_players();
    let want_mobs = cfg.want_mobs();
    if want_players || want_mobs {
        let mut range = 0.0_f32;
        if cfg.player.enabled {
            range = range.max(cfg.player.range);
        }
        if cfg.mob.enabled {
            range = range.max(cfg.mob.range);
        }
        if cfg.tracer.enabled {
            range = range.max(cfg.tracer.range);
        }
        if cfg.nametag.enabled {
            range = range.max(cfg.nametag.range);
        }
        let range_sq = (range as f64) * (range as f64);

        match gather_entities(range_sq, want_players, want_mobs) {
            Ok(list) => state.entities = list,
            Err(e) => {
                state.entities.clear();
                log::debug!("ESP: entity gather failed: {e}");
            }
        }
    } else {
        state.entities.clear();
    }

    if cfg.chest.enabled {
        let due = state
            .last_chest_scan
            .is_none_or(|t| now - t >= CHEST_SCAN_INTERVAL);
        if due {
            state.last_chest_scan = Some(now);
            match gather_chests() {
                Ok(list) => state.chests = list,
                Err(e) => log::debug!("ESP: chest scan failed: {e}"),
            }
        }
    } else {
        state.chests.clear();
        state.last_chest_scan = None;
    }
}

/// Walks `Level.entitiesForRendering()` once, classifying players and mobs.
fn gather_entities(
    range_sq: f64,
    want_player: bool,
    want_mob: bool,
) -> anyhow::Result<Vec<EntityTarget>> {
    let mc = minecraft();
    let (Some(world), Some(player)) = (mc.world()?, mc.player()?) else {
        return Ok(Vec::new());
    };

    let local_id = player.id()?;
    let player_pos: V3 = player.get_position()?.into();

    let mut out: Vec<EntityTarget> = Vec::new();
    for entity in world.get_entities()? {
        if let Some(target) = process_entity(
            &entity,
            local_id,
            player_pos,
            range_sq,
            want_player,
            want_mob,
        ) {
            out.push(target);
        }
    }

    Ok(out)
}

/// Turns one [`Entity`] into an [`EntityTarget`], or `None` if it is not a
/// wanted target. Errors are swallowed per-field so one bad entity cannot
/// abort the whole gather.
fn process_entity(
    entity: &Entity,
    local_id: i32,
    player_pos: V3,
    range_sq: f64,
    want_player: bool,
    want_mob: bool,
) -> Option<EntityTarget> {
    // Cheap distance gate first — a far entity then costs just this one JNI
    // call. Skipped entirely if `distanceToSqr` is not exposed by this build.
    if let Ok(dist_sq) = entity.distance_to_sqr(player_pos.x, player_pos.y, player_pos.z) {
        if dist_sq > range_sq {
            return None;
        }
    }

    let kind = if want_player && entity.instance_of::<Player>() {
        TargetKind::Player
    } else if want_mob && entity.instance_of::<Mob>() {
        TargetKind::Mob
    } else {
        return None;
    };

    let id = entity.id().ok()?;
    if id == local_id {
        return None;
    }

    let pos: V3 = entity.get_position().ok()?.into();
    let old = entity.get_old_position().map(V3::from).unwrap_or(pos);
    let width = entity.bb_width().ok()? as f64;
    let height = entity.bb_height().ok()? as f64;

    let name = read_name(entity);
    let (health, max_health) = read_health(entity).unwrap_or((0.0, 0.0));

    Some(EntityTarget {
        kind,
        old,
        pos,
        width,
        height,
        name,
        health,
        max_health,
    })
}

/// Reads an entity's display name, truncated to a sane label length.
fn read_name(entity: &Entity) -> String {
    let name = entity
        .get_name()
        .and_then(|component| component.get_string())
        .unwrap_or_default();
    if name.chars().count() > 24 {
        name.chars().take(24).collect()
    } else {
        name
    }
}

/// Reads `(health, maxHealth)`, or `None` if the entity is not living.
fn read_health(entity: &Entity) -> Option<(f32, f32)> {
    let living = entity.as_living()?;
    Some((living.get_health().ok()?, living.get_max_health().ok()?))
}

/// Scans loaded chunks around the player for container block entities.
fn gather_chests() -> anyhow::Result<Vec<ChestTarget>> {
    let mc = minecraft();
    let (Some(world), Some(player)) = (mc.world()?, mc.player()?) else {
        return Ok(Vec::new());
    };

    let player_pos: V3 = player.get_position()?.into();
    let pcx = (player_pos.x / 16.0).floor() as i32;
    let pcz = (player_pos.z / 16.0).floor() as i32;

    let mut out: Vec<ChestTarget> = Vec::new();
    for cx in (pcx - CHEST_CHUNK_RADIUS)..=(pcx + CHEST_CHUNK_RADIUS) {
        for cz in (pcz - CHEST_CHUNK_RADIUS)..=(pcz + CHEST_CHUNK_RADIUS) {
            scan_chunk(&world, cx, cz, &mut out)?;
        }
    }

    Ok(out)
}

/// Adds every container block entity of one chunk to `out`.
fn scan_chunk(world: &World, cx: i32, cz: i32, out: &mut Vec<ChestTarget>) -> anyhow::Result<()> {
    let Some(chunk) = world.get_chunk(cx, cz)? else {
        return Ok(());
    };

    for block_entity in chunk.get_block_entities()? {
        if !block_entity.is_container() {
            continue;
        }
        if let Ok(pos) = block_entity.get_block_pos() {
            out.push(ChestTarget {
                pos: V3 {
                    x: pos.x() as f64,
                    y: pos.y() as f64,
                    z: pos.z() as f64,
                },
            });
        }
    }
    Ok(())
}

// --- drawing ---------------------------------------------------------------

/// The 12 edges of a box, as index pairs into an 8-corner array.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0), // bottom
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4), // top
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7), // verticals
];

/// The 8 corners of an axis-aligned box `[min, max]`.
fn box_corners(min: V3, max: V3) -> [V3; 8] {
    [
        V3 {
            x: min.x,
            y: min.y,
            z: min.z,
        },
        V3 {
            x: max.x,
            y: min.y,
            z: min.z,
        },
        V3 {
            x: max.x,
            y: min.y,
            z: max.z,
        },
        V3 {
            x: min.x,
            y: min.y,
            z: max.z,
        },
        V3 {
            x: min.x,
            y: max.y,
            z: min.z,
        },
        V3 {
            x: max.x,
            y: max.y,
            z: min.z,
        },
        V3 {
            x: max.x,
            y: max.y,
            z: max.z,
        },
        V3 {
            x: min.x,
            y: max.y,
            z: max.z,
        },
    ]
}

/// Draws a wireframe box and returns its 2D screen bounds (for label
/// placement), or `None` if no corner is in front of the camera.
fn draw_wire_box(
    painter: &Painter,
    view: &View,
    corners: &[V3; 8],
    color: Color32,
) -> Option<Rect> {
    let projected: [Option<Pos2>; 8] = std::array::from_fn(|i| view.project(corners[i]));
    let stroke = Stroke::new(LINE_WIDTH, color);

    for &(a, b) in &EDGES {
        if let (Some(pa), Some(pb)) = (projected[a], projected[b]) {
            painter.line_segment([pa, pb], stroke);
        }
    }

    let mut bounds: Option<Rect> = None;
    for point in projected.into_iter().flatten() {
        bounds = Some(match bounds {
            Some(rect) => rect.union(Rect::from_min_max(point, point)),
            None => Rect::from_min_max(point, point),
        });
    }
    bounds
}

fn draw_entity(
    painter: &Painter,
    view: &View,
    entity: &EntityTarget,
    partial_tick: f64,
    cfg: &EspConfig,
) {
    let icfg = match entity.kind {
        TargetKind::Player => &cfg.player,
        TargetKind::Mob => &cfg.mob,
    };
    // The entity may have been gathered only for the tracers — skip the box if
    // the matching ESP category is itself disabled.
    if !icfg.enabled {
        return;
    }

    let feet = entity.old.lerp(entity.pos, partial_tick);
    let half = entity.width * 0.5;
    let corners = box_corners(
        V3 {
            x: feet.x - half,
            y: feet.y,
            z: feet.z - half,
        },
        V3 {
            x: feet.x + half,
            y: feet.y + entity.height,
            z: feet.z + half,
        },
    );

    let rect = match draw_wire_box(painter, view, &corners, icfg.color) {
        Some(rect) => rect,
        None => return,
    };

    if icfg.show_health && entity.max_health > 0.0 {
        draw_health_bar(painter, rect, entity.health / entity.max_health);
    }
    if icfg.show_name && !entity.name.is_empty() {
        draw_label(
            painter,
            pos2(rect.center().x, rect.top() - 7.0),
            Align2::CENTER_BOTTOM,
            &entity.name,
            icfg.color,
        );
    }
    if icfg.show_distance {
        let distance = feet.sub(view.cam).length();
        draw_label(
            painter,
            pos2(rect.center().x, rect.bottom() + 3.0),
            Align2::CENTER_TOP,
            &format!("{distance:.0}m"),
            Color32::from_rgb(214, 216, 224),
        );
    }
}

/// Draws a line from the bottom-centre of the screen to one entity. Skips the
/// entity if its kind is not wanted, it lies past the tracer range, or its
/// centre is behind the camera.
fn draw_tracer(
    painter: &Painter,
    view: &View,
    entity: &EntityTarget,
    partial_tick: f64,
    cfg: &TracerCfg,
) {
    let want = match entity.kind {
        TargetKind::Player => cfg.players,
        TargetKind::Mob => cfg.mobs,
    };
    if !want {
        return;
    }

    let feet = entity.old.lerp(entity.pos, partial_tick);
    let center = V3 {
        x: feet.x,
        y: feet.y + entity.height * 0.5,
        z: feet.z,
    };
    if center.sub(view.cam).length() > cfg.range as f64 {
        return;
    }

    let Some(target) = view.project(center) else {
        return;
    };
    let origin = pos2(view.w * 0.5, view.h);
    painter.line_segment([origin, target], Stroke::new(LINE_WIDTH, cfg.color));
}

/// Draws a floating name tag above one player — a background panel with the
/// player's name, optionally their health and distance. Skips non-players,
/// nameless entities, those past the range, or any behind the camera.
fn draw_nametag(
    painter: &Painter,
    view: &View,
    entity: &EntityTarget,
    partial_tick: f64,
    cfg: &NametagCfg,
) {
    if entity.kind != TargetKind::Player || entity.name.is_empty() {
        return;
    }

    let feet = entity.old.lerp(entity.pos, partial_tick);
    let head = V3 {
        x: feet.x,
        y: feet.y + entity.height + 0.45,
        z: feet.z,
    };
    if head.sub(view.cam).length() > cfg.range as f64 {
        return;
    }
    let Some(anchor) = view.project(head) else {
        return;
    };

    let mut label = entity.name.clone();
    if cfg.show_health && entity.max_health > 0.0 {
        label.push_str(&format!("  {:.0}\u{2764}", entity.health.max(0.0)));
    }
    if cfg.show_distance {
        label.push_str(&format!("  {:.0}m", feet.sub(view.cam).length()));
    }

    let galley = painter.layout_no_wrap(label, FontId::proportional(12.0), cfg.color);
    let pad = vec2(5.0, 2.5);
    let size = galley.size() + pad * 2.0;
    // The panel sits centred on, and just above, the anchor point.
    let rect = Rect::from_min_size(pos2(anchor.x - size.x * 0.5, anchor.y - size.y), size);
    painter.rect_filled(rect, Rounding::same(3.0), Color32::from_black_alpha(180));
    painter.galley(rect.min + pad, galley, cfg.color);
}

fn draw_chest(painter: &Painter, view: &View, chest: &ChestTarget, cfg: &EspConfig) {
    let corners = box_corners(
        chest.pos,
        V3 {
            x: chest.pos.x + 1.0,
            y: chest.pos.y + 1.0,
            z: chest.pos.z + 1.0,
        },
    );
    let rect = match draw_wire_box(painter, view, &corners, cfg.chest.color) {
        Some(rect) => rect,
        None => return,
    };

    if cfg.chest.show_distance {
        let center = V3 {
            x: chest.pos.x + 0.5,
            y: chest.pos.y + 0.5,
            z: chest.pos.z + 0.5,
        };
        let distance = center.sub(view.cam).length();
        draw_label(
            painter,
            pos2(rect.center().x, rect.bottom() + 3.0),
            Align2::CENTER_TOP,
            &format!("{distance:.0}m"),
            cfg.chest.color,
        );
    }
}

/// A thin health bar straddling the top edge of `rect`, red→yellow→green.
fn draw_health_bar(painter: &Painter, rect: Rect, fraction: f32) {
    let fraction = fraction.clamp(0.0, 1.0);
    let top = rect.top() - 4.0;
    let bottom = rect.top() - 1.5;
    let background = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), bottom));
    painter.rect_filled(background, Rounding::ZERO, Color32::from_black_alpha(190));

    let fill_width = rect.width() * fraction;
    let fill = Rect::from_min_max(
        pos2(rect.left(), top),
        pos2(rect.left() + fill_width, bottom),
    );
    painter.rect_filled(fill, Rounding::ZERO, health_color(fraction));
}

/// Interpolates red → yellow → green across `0..=1`.
fn health_color(fraction: f32) -> Color32 {
    let (r, g) = if fraction < 0.5 {
        (255.0, 255.0 * (fraction * 2.0))
    } else {
        (255.0 * (1.0 - (fraction - 0.5) * 2.0), 255.0)
    };
    Color32::from_rgb(r as u8, g as u8, 60)
}

/// Draws text with a 1px drop shadow so it stays readable over any backdrop.
fn draw_label(painter: &Painter, pos: Pos2, anchor: Align2, text: &str, color: Color32) {
    let font = FontId::proportional(11.0);
    painter.text(
        pos + vec2(0.8, 0.8),
        anchor,
        text,
        font.clone(),
        Color32::from_black_alpha(210),
    );
    painter.text(pos, anchor, text, font, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3(x: f64, y: f64, z: f64) -> V3 {
        V3 { x, y, z }
    }

    #[test]
    fn v3_length_and_dot() {
        let a = v3(3.0, 4.0, 0.0);
        assert_eq!(a.length(), 5.0);
        assert_eq!(a.dot(a), 25.0);
    }

    #[test]
    fn v3_cross_of_x_and_y_is_z() {
        let z = v3(1.0, 0.0, 0.0).cross(v3(0.0, 1.0, 0.0));
        assert!(z.sub(v3(0.0, 0.0, 1.0)).length() < 1e-9);
    }

    #[test]
    fn v3_lerp_finds_the_midpoint() {
        let m = v3(0.0, 0.0, 0.0).lerp(v3(10.0, 20.0, -4.0), 0.5);
        assert_eq!((m.x, m.y, m.z), (5.0, 10.0, -2.0));
    }

    #[test]
    fn a_point_dead_ahead_projects_to_the_screen_centre() {
        // Camera at the origin, yaw/pitch 0 -> looking toward +Z.
        let view = build_view(
            v3(0.0, 0.0, 0.0),
            0.0,
            0.0,
            70.0,
            1920.0,
            1080.0,
            Affine::IDENTITY,
        );
        let projected = view
            .project(v3(0.0, 0.0, 10.0))
            .expect("a point straight ahead must project");
        assert!((projected.x - 960.0).abs() < 1.0);
        assert!((projected.y - 540.0).abs() < 1.0);
    }

    #[test]
    fn a_point_behind_the_camera_does_not_project() {
        let view = build_view(
            v3(0.0, 0.0, 0.0),
            0.0,
            0.0,
            70.0,
            1920.0,
            1080.0,
            Affine::IDENTITY,
        );
        assert!(view.project(v3(0.0, 0.0, -10.0)).is_none());
    }

    #[test]
    fn affine_mul_applies_the_right_operand_first() {
        // Translate along x, then rotate 90° about z: x ends up on y.
        let pose = Affine::rotation_z(90.0).mul(Affine::translation(1.0, 0.0, 0.0));
        let [x, y, z] = pose.apply([0.0, 0.0, 0.0]);
        assert!(x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9 && z.abs() < 1e-9);
    }

    #[test]
    fn a_still_unhurt_player_has_no_bob() {
        let bob = CameraBob {
            view_bobbing: true,
            is_player: true,
            is_living: true,
            hurt_time: -0.5,
            ..CameraBob::default()
        };
        let [x, y, z] = bob_transform(&bob).apply([1.0, 2.0, 3.0]);
        assert!((x - 1.0).abs() < 1e-9 && (y - 2.0).abs() < 1e-9 && (z - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_bob_sway_to_the_right_moves_projections_right() {
        let bob = Affine::translation(0.5, 0.0, 0.0);
        let view = build_view(v3(0.0, 0.0, 0.0), 0.0, 0.0, 70.0, 1920.0, 1080.0, bob);
        let projected = view.project(v3(0.0, 0.0, 10.0)).unwrap();
        assert!(projected.x > 960.0 + 1.0);
    }

    #[test]
    fn box_corners_span_min_to_max() {
        let corners = box_corners(v3(0.0, 0.0, 0.0), v3(1.0, 2.0, 3.0));
        assert_eq!(corners.len(), 8);
        assert_eq!((corners[0].x, corners[0].y, corners[0].z), (0.0, 0.0, 0.0));
        assert_eq!((corners[6].x, corners[6].y, corners[6].z), (1.0, 2.0, 3.0));
    }
}
