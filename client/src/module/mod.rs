use crate::mapping::entity::player::LocalPlayer;
use std::fmt::Debug;

pub mod fly;

pub type ModuleType = dyn Module + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleCategory {
    COMBAT,
    MOVEMENT,
    RENDER,
    PLAYER,
    WORLD,
    MISC,
}

impl ModuleCategory {
    pub fn display_name(&self) -> &str {
        match self {
            ModuleCategory::COMBAT => "Combat",
            ModuleCategory::MOVEMENT => "Movement",
            ModuleCategory::RENDER => "Render",
            ModuleCategory::PLAYER => "Player",
            ModuleCategory::WORLD => "World",
            ModuleCategory::MISC => "Misc",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleData {
    pub name: String,
    pub description: String,
    pub category: ModuleCategory,
    pub key_bind: KeyboardKey,
    pub enabled: bool,
    pub player: LocalPlayer,
}

#[derive(Debug, Clone)]
pub enum ModuleSetting {
    Toggle {
        name: String,
        value: bool,
    },
    Slider {
        name: String,
        value: f32,
        min: f32,
        max: f32,
    },
    Choice {
        name: String,
        value: usize,
        options: Vec<String>,
    },
    Color {
        name: String,
        value: [f32; 4],
    },
}

impl ModuleData {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

pub trait Module: Debug + Send + Sync {
    fn on_start(&self);
    fn on_stop(&self);
    fn on_tick(&self);

    fn get_module_data(&self) -> &ModuleData;
    fn get_module_data_mut(&mut self) -> &mut ModuleData;
}

#[derive(Debug)]
pub struct FlyModule {
    pub module: ModuleData,
}

// lwjgl key mapping
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum KeyboardKey {
    KeyNone = -1,
    KeyEscape = 256,
    Key1 = 49,
    Key2 = 50,
    Key3 = 51,
    Key4 = 52,
    Key5 = 53,
    Key6 = 54,
    Key7 = 55,
    Key8 = 56,
    Key9 = 57,
    Key0 = 48,
    KeyMinus = 45,
    KeyEquals = 61,
    KeyBack = 259,
    KeyTab = 258,
    KeyQ = 81,
    KeyW = 87,
    KeyE = 69,
    KeyR = 82,
    KeyT = 84,
    KeyY = 89,
    KeyU = 85,
    KeyI = 73,
    KeyO = 79,
    KeyP = 80,
    KeyLBracket = 91,
    KeyRBracket = 93,
    KeyReturn = 257,
    KeyLControl = 341,
    KeyA = 65,
    KeyS = 83,
    KeyD = 68,
    KeyF = 70,
    KeyG = 71,
    KeyH = 72,
    KeyJ = 74,
    KeyK = 75,
    KeyL = 76,
    KeySemicolon = 59,
    KeyApostrophe = 39,
    KeyGrave = 96,
    KeyLShift = 340,
    KeyBackSlash = 92,
    KeyZ = 90,
    KeyX = 88,
    KeyC = 67,
    KeyV = 86,
    KeyB = 66,
    KeyN = 78,
    KeyM = 77,
    KeyComma = 44,
    KeyPeriod = 46,
    KeySlash = 47,
    KeyRShift = 344,
    KeyMultiply = 332,
    KeyLAlt = 342,
    KeySpace = 32,
    KeyCapital = 280,
    KeyF1 = 290,
    KeyF2 = 291,
    KeyF3 = 292,
    KeyF4 = 293,
    KeyF5 = 294,
    KeyF6 = 295,
    KeyF7 = 296,
    KeyF8 = 297,
    KeyF9 = 298,
    KeyF10 = 299,
    KeyNumLock = 282,
    KeyScroll = 281,
    KeyNumpad7 = 327,
    KeyNumpad8 = 328,
    KeyNumpad9 = 329,
    KeySubtract = 333,
    KeyNumpad4 = 324,
    KeyNumpad5 = 325,
    KeyNumpad6 = 326,
    KeyAdd = 334,
    KeyNumpad1 = 321,
    KeyNumpad2 = 322,
    KeyNumpad3 = 323,
    KeyNumpad0 = 320,
    KeyF11 = 300,
    KeyF12 = 301,
    KeyF13 = 302,
    KeyF14 = 303,
    KeyF15 = 304,
    KeyF16 = 305,
    KeyF17 = 306,
    KeyF18 = 307,
    KeyF19 = 308,
    KeyNumpadEquals = 336,
    KeyNumpadEnter = 335,
    KeyRControl = 345,
    KeyNumpadComma = 330,
    KeyDivide = 331,
    KeyPause = 284,
    KeyHome = 268,
    KeyUp = 265,
    KeyLeft = 263,
    KeyRight = 262,
    KeyEnd = 269,
    KeyDown = 264,
    KeyNext = 267,
    KeyInsert = 260,
    KeyDelete = 261,
}
