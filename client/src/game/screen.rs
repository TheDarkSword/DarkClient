use mc_bindings::{
    AbstractContainerScreen, ChatScreen, CraftingScreen, CreativeModeInventoryScreen,
    InventoryScreen, Minecraft, PauseScreen,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    None,
    Chat,
    Inventory,
    Container,
    Crafting,
    Menu,
    Unknown,
}

impl ScreenKind {
    pub fn is_open(self) -> bool {
        self != ScreenKind::None
    }
}

pub trait MinecraftExt {
    /// A failed read reports `Unknown`, which counts as open.
    fn current_screen(&self) -> ScreenKind;

    fn in_world(&self) -> bool;
}

impl MinecraftExt for Minecraft {
    fn current_screen(&self) -> ScreenKind {
        let read = || -> mc_bindings::Result<ScreenKind> {
            let Some(screen) = self.gui()?.screen()? else {
                return Ok(ScreenKind::None);
            };
            // Specific screens first: they extend `AbstractContainerScreen`.
            Ok(if screen.is::<ChatScreen>()? {
                ScreenKind::Chat
            } else if screen.is::<CreativeModeInventoryScreen>()?
                || screen.is::<InventoryScreen>()?
            {
                ScreenKind::Inventory
            } else if screen.is::<CraftingScreen>()? {
                ScreenKind::Crafting
            } else if screen.is::<AbstractContainerScreen>()? {
                ScreenKind::Container
            } else if screen.is::<PauseScreen>()? {
                ScreenKind::Menu
            } else {
                ScreenKind::Unknown
            })
        };
        read().unwrap_or(ScreenKind::Unknown)
    }

    fn in_world(&self) -> bool {
        matches!(self.player(), Ok(Some(_)))
    }
}
