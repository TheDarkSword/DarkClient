use crate::mapping::client::delta_tracker::DeltaTracker;
use crate::mapping::client::game_renderer::GameRenderer;
use crate::mapping::client::gamemode::MultiPlayerGameMode;
use crate::mapping::client::options::Options;
use crate::mapping::client::screen::Screen;
use crate::mapping::client::window::Window;
use crate::mapping::client::world::World;
use crate::mapping::entity::player::LocalPlayer;
use crate::mapping::MinecraftVersion;
use crate::mapping::{FieldType, MappedObject, MinecraftClassType};
use crate::state::mapping;
use jni::objects::{GlobalRef, JObject, JValue};
use std::sync::RwLock;

/// The running Minecraft client.
///
/// Only the things that exist from the main menu onward are held eagerly: the
/// `Minecraft.getInstance()` handle and the game [`Window`]. The world-scoped
/// objects — player, level, game mode — are null in the menu and on world
/// exit, so they are fetched lazily and reported as `Ok(None)` when absent.
/// This is what lets the client be injected from the main menu.
#[derive(Debug, MappedObject)]
#[mapped(class = Minecraft)]
pub struct Minecraft {
    jni_ref: GlobalRef,
    pub window: Window,
    /// Cached local player, refreshed when the underlying JVM object changes.
    player: RwLock<Option<LocalPlayer>>,
}

impl Minecraft {
    /// Builds the game wrapper from the live `Minecraft.getInstance()`.
    /// Succeeds whether or not a world is loaded.
    pub fn new() -> anyhow::Result<Minecraft> {
        let jni_ref = mapping().in_frame(|| {
            let minecraft = mapping()
                .call_static_method(MinecraftClassType::Minecraft, "getInstance", &[])?
                .l()?;
            if minecraft.is_null() {
                return Err(anyhow::anyhow!("Minecraft.getInstance() returned null"));
            }
            mapping().new_global_ref(minecraft)
        })?;
        let window = Window::new(&jni_ref)?;

        Ok(Minecraft {
            jni_ref,
            window,
            player: RwLock::new(None),
        })
    }

    /// The local player, or `Ok(None)` when not in a world.
    ///
    /// The result is cached and refreshed when the underlying JVM object
    /// changes (a new world join produces a fresh instance).
    pub fn player(&self) -> anyhow::Result<Option<LocalPlayer>> {
        let Some(player_ref) = self.world_object("player", MinecraftClassType::LocalPlayer)? else {
            // Left the world: drop the cached `LocalPlayer` so its JVM object
            // is no longer pinned by a global reference until the next join.
            if let Ok(mut cache) = self.player.write() {
                *cache = None;
            }
            return Ok(None);
        };

        {
            let cache = self
                .player
                .read()
                .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            if let Some(cached) = cache.as_ref() {
                if mapping()
                    .get_env()?
                    .is_same_object(cached.jni_ref(), &player_ref)?
                {
                    return Ok(Some(cached.clone()));
                }
            }
        }

        let player = LocalPlayer::new(player_ref)?;
        *self
            .player
            .write()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))? = Some(player.clone());
        Ok(Some(player))
    }

    /// The current world / level, or `Ok(None)` when not in a world.
    pub fn world(&self) -> anyhow::Result<Option<World>> {
        Ok(self
            .world_object("level", MinecraftClassType::Level)?
            .map(World::new))
    }

    /// The interaction controller, or `Ok(None)` when not in a world.
    pub fn game_mode(&self) -> anyhow::Result<Option<MultiPlayerGameMode>> {
        Ok(self
            .world_object("gameMode", MinecraftClassType::MultiPlayerGameMode)?
            .map(MultiPlayerGameMode::new))
    }

    /// The game renderer. Present from the main menu onward.
    pub fn game_renderer(&self) -> anyhow::Result<GameRenderer> {
        self.in_frame(|| {
            let obj = self
                .get_field(
                    "gameRenderer",
                    FieldType::Object(MinecraftClassType::GameRenderer),
                )?
                .l()?;
            Ok(GameRenderer::new(mapping().new_global_ref(obj)?))
        })
    }

    /// The frame timer. Present from the main menu onward.
    pub fn delta_tracker(&self) -> anyhow::Result<DeltaTracker> {
        self.in_frame(|| {
            let obj = self.call_method("getDeltaTracker", &[])?.l()?;
            Ok(DeltaTracker::new(mapping().new_global_ref(obj)?))
        })
    }

    /// Puts `text` on the system clipboard — through Minecraft, so it works
    /// whatever windowing library the game runs on.
    pub fn set_clipboard(&self, text: &str) -> anyhow::Result<()> {
        self.in_frame(|| {
            let handler = self.keyboard_handler()?;
            let text = mapping().get_env()?.new_string(text)?;
            mapping().call_method(
                MinecraftClassType::KeyboardHandler,
                &handler,
                "setClipboard",
                &[JValue::Object(&text)],
            )?;
            Ok(())
        })
    }

    /// The text on the system clipboard.
    pub fn clipboard(&self) -> anyhow::Result<String> {
        self.in_frame(|| {
            let handler = self.keyboard_handler()?;
            let text = mapping()
                .call_method(
                    MinecraftClassType::KeyboardHandler,
                    &handler,
                    "getClipboard",
                    &[],
                )?
                .l()?;
            mapping().get_string(text)
        })
    }

    fn keyboard_handler(&self) -> anyhow::Result<JObject<'static>> {
        Ok(self
            .get_field(
                "keyboardHandler",
                FieldType::Object(MinecraftClassType::KeyboardHandler),
            )?
            .l()?)
    }

    /// The game options. Present from the main menu onward.
    pub fn options(&self) -> anyhow::Result<Options> {
        self.in_frame(|| {
            let obj = self
                .get_field("options", FieldType::Object(MinecraftClassType::Options))?
                .l()?;
            Ok(Options::new(mapping().new_global_ref(obj)?))
        })
    }

    /// Whether a world is currently loaded.
    pub fn in_world(&self) -> bool {
        matches!(self.player(), Ok(Some(_)))
    }

    /// Drops the cached local player — a held global reference. Called from
    /// `cleanup_client` before the library is unloaded.
    pub fn teardown(&self) {
        if let Ok(mut cache) = self.player.write() {
            *cache = None;
        }
    }

    /// Whether no screen (menu / inventory / …) is currently open.
    pub fn current_screen_is_null(&self) -> bool {
        self.current_screen() == Screen::None
    }

    /// The Minecraft screen currently open. Best-effort: any JNI failure — or
    /// an unrecognized screen — reports [`Screen::Unknown`], which callers
    /// treat as "a screen is open".
    pub fn current_screen(&self) -> Screen {
        self.in_frame(|| {
            let screen = if mapping().get_version() >= MinecraftVersion::new(26, 2, 0) {
                // 26.2 moved the open screen from `Minecraft` into `Gui`.
                let gui = self
                    .get_field("gui", FieldType::Object(MinecraftClassType::Gui))?
                    .l()?;
                mapping()
                    .call_method(MinecraftClassType::Gui, &gui, "screen", &[])?
                    .l()?
            } else {
                self.get_field("screen", FieldType::Object(MinecraftClassType::Screen))?
                    .l()?
            };
            if screen.is_null() {
                return Ok(Screen::None);
            }

            let is = |class| mapping().is_instance_of(class, &screen).unwrap_or(false);
            // Specific screens first — `InventoryScreen` / `CraftingScreen`
            // both extend `AbstractContainerScreen`.
            let screen = if is(MinecraftClassType::ChatScreen) {
                Screen::Chat
            } else if is(MinecraftClassType::CreativeModeInventoryScreen)
                || is(MinecraftClassType::InventoryScreen)
            {
                Screen::Inventory
            } else if is(MinecraftClassType::CraftingScreen) {
                Screen::Crafting
            } else if is(MinecraftClassType::AbstractContainerScreen) {
                Screen::Container
            } else if is(MinecraftClassType::PauseScreen) {
                Screen::Menu
            } else {
                Screen::Unknown
            };
            Ok(screen)
        })
        .unwrap_or(Screen::Unknown)
    }

    /// Reads a world-scoped object field of `Minecraft`, returning `Ok(None)`
    /// when it is null — i.e. when there is no world loaded.
    fn world_object(
        &self,
        field: &str,
        class: MinecraftClassType,
    ) -> anyhow::Result<Option<GlobalRef>> {
        self.in_frame(|| {
            let obj = self.get_field(field, FieldType::Object(class))?.l()?;
            if obj.is_null() {
                return Ok(None);
            }
            Ok(Some(mapping().new_global_ref(obj)?))
        })
    }
}
