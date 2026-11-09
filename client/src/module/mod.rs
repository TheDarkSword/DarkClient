use std::fmt::Debug;

pub mod combat;
pub mod misc;
pub mod movement;
pub mod player;
pub mod registry;
pub mod render;
pub mod world;

pub type ModuleType = Box<dyn Module + Send + Sync>;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ModuleCategory {
    Combat,
    Movement,
    Render,
    Player,
    World,
    Misc,
}

impl ModuleCategory {
    #[allow(dead_code)]
    pub fn display_name(&self) -> &str {
        match self {
            ModuleCategory::Combat => "Combat",
            ModuleCategory::Movement => "Movement",
            ModuleCategory::Render => "Render",
            ModuleCategory::Player => "Player",
            ModuleCategory::World => "World",
            ModuleCategory::Misc => "Misc",
        }
    }
}

/// Stable identifier of a module — what it is registered and looked up by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ModuleId {
    Fly,
    NoFall,
    KillAura,
    MobAura,
    Aimbot,
    Velocity,
    PlayerEsp,
    MobEsp,
    ChestEsp,
    Tracers,
    Fullbright,
    AntiAfk,
    Sprint,
    Nametags,
    Criticals,
    Speed,
    NoSlowdown,
    AutoTotem,
    ChestStealer,
    AutoArmor,
    Nuker,
    Scaffold,
    Freecam,
}

impl ModuleId {
    /// Human-readable name, shown in the UI.
    pub fn display_name(self) -> &'static str {
        match self {
            ModuleId::Fly => "Fly",
            ModuleId::NoFall => "NoFall",
            ModuleId::KillAura => "KillAura",
            ModuleId::MobAura => "MobAura",
            ModuleId::Aimbot => "Aimbot",
            ModuleId::Velocity => "Velocity",
            ModuleId::PlayerEsp => "Player ESP",
            ModuleId::MobEsp => "Mob ESP",
            ModuleId::ChestEsp => "Chest ESP",
            ModuleId::Tracers => "Tracers",
            ModuleId::Fullbright => "Fullbright",
            ModuleId::AntiAfk => "AntiAFK",
            ModuleId::Sprint => "Sprint",
            ModuleId::Nametags => "Nametags",
            ModuleId::Criticals => "Criticals",
            ModuleId::Speed => "Speed",
            ModuleId::NoSlowdown => "NoSlowdown",
            ModuleId::AutoTotem => "AutoTotem",
            ModuleId::ChestStealer => "ChestStealer",
            ModuleId::AutoArmor => "AutoArmor",
            ModuleId::Nuker => "Nuker",
            ModuleId::Scaffold => "Scaffold",
            ModuleId::Freecam => "Freecam",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleData {
    pub id: ModuleId,
    #[allow(dead_code)]
    pub description: String,
    #[allow(dead_code)]
    pub category: ModuleCategory,
    pub key_bind: KeyboardKey,
    pub enabled: bool,
    pub settings: Vec<ModuleSetting>,
}

#[allow(dead_code)]
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
        /// sRGB components and alpha, unmultiplied, each `0.0..=1.0`.
        value: [f32; 4],
        /// Whether the picker offers the alpha channel — only for colors
        /// whose consumer actually draws with it.
        alpha: bool,
    },
}

impl ModuleSetting {
    pub fn name(&self) -> &str {
        match self {
            ModuleSetting::Toggle { name, .. } => name,
            ModuleSetting::Slider { name, .. } => name,
            ModuleSetting::Choice { name, .. } => name,
            ModuleSetting::Color { name, .. } => name,
        }
    }

    pub fn get_slider_value(&self) -> Option<f32> {
        match self {
            ModuleSetting::Slider { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn set_slider_value(&mut self, new_value: f32) {
        if let ModuleSetting::Slider { value, .. } = self {
            *value = new_value;
        }
    }

    pub fn get_toggle_value(&self) -> Option<bool> {
        match self {
            ModuleSetting::Toggle { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn set_toggle_value(&mut self, new_value: bool) {
        if let ModuleSetting::Toggle { value, .. } = self {
            *value = new_value;
        }
    }
}

impl ModuleData {
    /// The module's display name.
    pub fn name(&self) -> &'static str {
        self.id.display_name()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn get_setting_mut(&mut self, name: &str) -> Option<&mut ModuleSetting> {
        self.settings.iter_mut().find(|s| s.name() == name)
    }

    pub fn get_setting(&self, name: &str) -> Option<&ModuleSetting> {
        self.settings.iter().find(|s| s.name() == name)
    }
}

pub trait Module: Debug + Send + Sync {
    fn on_start(&self) -> anyhow::Result<()>;
    fn on_stop(&self) -> anyhow::Result<()>;
    fn on_tick(&self) -> anyhow::Result<()>;

    /// Inspects — and may modify — a packet passing through the connection.
    /// Only enabled modules are called. Mutate `packet` in place to rewrite it;
    /// return [`PacketAction::Cancel`] to drop it entirely. The default ignores
    /// every packet and forwards it untouched.
    fn handle_packet(
        &self,
        _packet: &mut crate::net::packet::Packet,
    ) -> crate::net::packet::PacketAction {
        crate::net::packet::PacketAction::Forward
    }

    fn get_module_data(&self) -> &ModuleData;
    fn get_module_data_mut(&mut self) -> &mut ModuleData;
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

impl KeyboardKey {
    pub fn from(key: i32) -> Self {
        match key {
            -1 => KeyboardKey::KeyNone,
            256 => KeyboardKey::KeyEscape,
            49 => KeyboardKey::Key1,
            50 => KeyboardKey::Key2,
            51 => KeyboardKey::Key3,
            52 => KeyboardKey::Key4,
            53 => KeyboardKey::Key5,
            54 => KeyboardKey::Key6,
            55 => KeyboardKey::Key7,
            56 => KeyboardKey::Key8,
            57 => KeyboardKey::Key9,
            48 => KeyboardKey::Key0,
            45 => KeyboardKey::KeyMinus,
            61 => KeyboardKey::KeyEquals,
            259 => KeyboardKey::KeyBack,
            258 => KeyboardKey::KeyTab,
            81 => KeyboardKey::KeyQ,
            87 => KeyboardKey::KeyW,
            69 => KeyboardKey::KeyE,
            82 => KeyboardKey::KeyR,
            84 => KeyboardKey::KeyT,
            89 => KeyboardKey::KeyY,
            85 => KeyboardKey::KeyU,
            73 => KeyboardKey::KeyI,
            79 => KeyboardKey::KeyO,
            80 => KeyboardKey::KeyP,
            91 => KeyboardKey::KeyLBracket,
            93 => KeyboardKey::KeyRBracket,
            257 => KeyboardKey::KeyReturn,
            341 => KeyboardKey::KeyLControl,
            65 => KeyboardKey::KeyA,
            83 => KeyboardKey::KeyS,
            68 => KeyboardKey::KeyD,
            70 => KeyboardKey::KeyF,
            71 => KeyboardKey::KeyG,
            72 => KeyboardKey::KeyH,
            74 => KeyboardKey::KeyJ,
            75 => KeyboardKey::KeyK,
            76 => KeyboardKey::KeyL,
            59 => KeyboardKey::KeySemicolon,
            39 => KeyboardKey::KeyApostrophe,
            96 => KeyboardKey::KeyGrave,
            340 => KeyboardKey::KeyLShift,
            92 => KeyboardKey::KeyBackSlash,
            90 => KeyboardKey::KeyZ,
            88 => KeyboardKey::KeyX,
            67 => KeyboardKey::KeyC,
            86 => KeyboardKey::KeyV,
            66 => KeyboardKey::KeyB,
            78 => KeyboardKey::KeyN,
            77 => KeyboardKey::KeyM,
            44 => KeyboardKey::KeyComma,
            46 => KeyboardKey::KeyPeriod,
            47 => KeyboardKey::KeySlash,
            344 => KeyboardKey::KeyRShift,
            332 => KeyboardKey::KeyMultiply,
            342 => KeyboardKey::KeyLAlt,
            32 => KeyboardKey::KeySpace,
            280 => KeyboardKey::KeyCapital,
            290 => KeyboardKey::KeyF1,
            291 => KeyboardKey::KeyF2,
            292 => KeyboardKey::KeyF3,
            293 => KeyboardKey::KeyF4,
            294 => KeyboardKey::KeyF5,
            295 => KeyboardKey::KeyF6,
            296 => KeyboardKey::KeyF7,
            297 => KeyboardKey::KeyF8,
            298 => KeyboardKey::KeyF9,
            299 => KeyboardKey::KeyF10,
            282 => KeyboardKey::KeyNumLock,
            281 => KeyboardKey::KeyScroll,
            327 => KeyboardKey::KeyNumpad7,
            328 => KeyboardKey::KeyNumpad8,
            329 => KeyboardKey::KeyNumpad9,
            333 => KeyboardKey::KeySubtract,
            324 => KeyboardKey::KeyNumpad4,
            325 => KeyboardKey::KeyNumpad5,
            326 => KeyboardKey::KeyNumpad6,
            334 => KeyboardKey::KeyAdd,
            321 => KeyboardKey::KeyNumpad1,
            322 => KeyboardKey::KeyNumpad2,
            323 => KeyboardKey::KeyNumpad3,
            320 => KeyboardKey::KeyNumpad0,
            300 => KeyboardKey::KeyF11,
            301 => KeyboardKey::KeyF12,
            302 => KeyboardKey::KeyF13,
            303 => KeyboardKey::KeyF14,
            304 => KeyboardKey::KeyF15,
            305 => KeyboardKey::KeyF16,
            306 => KeyboardKey::KeyF17,
            307 => KeyboardKey::KeyF18,
            308 => KeyboardKey::KeyF19,
            336 => KeyboardKey::KeyNumpadEquals,
            335 => KeyboardKey::KeyNumpadEnter,
            345 => KeyboardKey::KeyRControl,
            330 => KeyboardKey::KeyNumpadComma,
            331 => KeyboardKey::KeyDivide,
            284 => KeyboardKey::KeyPause,
            268 => KeyboardKey::KeyHome,
            265 => KeyboardKey::KeyUp,
            263 => KeyboardKey::KeyLeft,
            262 => KeyboardKey::KeyRight,
            269 => KeyboardKey::KeyEnd,
            264 => KeyboardKey::KeyDown,
            267 => KeyboardKey::KeyNext,
            260 => KeyboardKey::KeyInsert,
            261 => KeyboardKey::KeyDelete,
            _ => KeyboardKey::KeyNone,
        }
    }
}

impl std::fmt::Display for KeyboardKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            KeyboardKey::KeyNone => str::to_string("None"),
            KeyboardKey::KeyEscape => str::to_string("ESC"),
            KeyboardKey::Key1 => str::to_string("1"),
            KeyboardKey::Key2 => str::to_string("2"),
            KeyboardKey::Key3 => str::to_string("3"),
            KeyboardKey::Key4 => str::to_string("4"),
            KeyboardKey::Key5 => str::to_string("5"),
            KeyboardKey::Key6 => str::to_string("6"),
            KeyboardKey::Key7 => str::to_string("7"),
            KeyboardKey::Key8 => str::to_string("8"),
            KeyboardKey::Key9 => str::to_string("9"),
            KeyboardKey::Key0 => str::to_string("0"),
            KeyboardKey::KeyMinus => str::to_string("Minus"),
            KeyboardKey::KeyEquals => str::to_string("Equals"),
            KeyboardKey::KeyBack => str::to_string("Back"),
            KeyboardKey::KeyTab => str::to_string("Tab"),
            KeyboardKey::KeyQ => str::to_string("Q"),
            KeyboardKey::KeyW => str::to_string("W"),
            KeyboardKey::KeyE => str::to_string("E"),
            KeyboardKey::KeyR => str::to_string("R"),
            KeyboardKey::KeyT => str::to_string("T"),
            KeyboardKey::KeyY => str::to_string("Y"),
            KeyboardKey::KeyU => str::to_string("U"),
            KeyboardKey::KeyI => str::to_string("I"),
            KeyboardKey::KeyO => str::to_string("O"),
            KeyboardKey::KeyP => str::to_string("P"),
            KeyboardKey::KeyLBracket => str::to_string("LBracket"),
            KeyboardKey::KeyRBracket => str::to_string("RBracket"),
            KeyboardKey::KeyReturn => str::to_string("Return"),
            KeyboardKey::KeyLControl => str::to_string("LControl"),
            KeyboardKey::KeyA => str::to_string("A"),
            KeyboardKey::KeyS => str::to_string("S"),
            KeyboardKey::KeyD => str::to_string("D"),
            KeyboardKey::KeyF => str::to_string("F"),
            KeyboardKey::KeyG => str::to_string("G"),
            KeyboardKey::KeyH => str::to_string("H"),
            KeyboardKey::KeyJ => str::to_string("J"),
            KeyboardKey::KeyK => str::to_string("K"),
            KeyboardKey::KeyL => str::to_string("L"),
            KeyboardKey::KeySemicolon => str::to_string("Semicolon"),
            KeyboardKey::KeyApostrophe => str::to_string("Apostrophe"),
            KeyboardKey::KeyGrave => str::to_string("Grave"),
            KeyboardKey::KeyLShift => str::to_string("LShift"),
            KeyboardKey::KeyBackSlash => str::to_string("BackSlash"),
            KeyboardKey::KeyZ => str::to_string("Z"),
            KeyboardKey::KeyX => str::to_string("X"),
            KeyboardKey::KeyC => str::to_string("C"),
            KeyboardKey::KeyV => str::to_string("V"),
            KeyboardKey::KeyB => str::to_string("B"),
            KeyboardKey::KeyN => str::to_string("N"),
            KeyboardKey::KeyM => str::to_string("M"),
            KeyboardKey::KeyComma => str::to_string("Comma"),
            KeyboardKey::KeyPeriod => str::to_string("Period"),
            KeyboardKey::KeySlash => str::to_string("Slash"),
            KeyboardKey::KeyRShift => str::to_string("RShift"),
            KeyboardKey::KeyMultiply => str::to_string("Multiply"),
            KeyboardKey::KeyLAlt => str::to_string("LAlt"),
            KeyboardKey::KeySpace => str::to_string("Space"),
            KeyboardKey::KeyCapital => str::to_string("Capital"),
            KeyboardKey::KeyF1 => str::to_string("F1"),
            KeyboardKey::KeyF2 => str::to_string("F2"),
            KeyboardKey::KeyF3 => str::to_string("F3"),
            KeyboardKey::KeyF4 => str::to_string("F4"),
            KeyboardKey::KeyF5 => str::to_string("F5"),
            KeyboardKey::KeyF6 => str::to_string("F6"),
            KeyboardKey::KeyF7 => str::to_string("F7"),
            KeyboardKey::KeyF8 => str::to_string("F8"),
            KeyboardKey::KeyF9 => str::to_string("F9"),
            KeyboardKey::KeyF10 => str::to_string("F10"),
            KeyboardKey::KeyNumLock => str::to_string("NumLock"),
            KeyboardKey::KeyScroll => str::to_string("Scroll"),
            KeyboardKey::KeyNumpad7 => str::to_string("Numpad7"),
            KeyboardKey::KeyNumpad8 => str::to_string("Numpad8"),
            KeyboardKey::KeyNumpad9 => str::to_string("Numpad9"),
            KeyboardKey::KeySubtract => str::to_string("Subtract"),
            KeyboardKey::KeyNumpad4 => str::to_string("Numpad4"),
            KeyboardKey::KeyNumpad5 => str::to_string("Numpad5"),
            KeyboardKey::KeyNumpad6 => str::to_string("Numpad6"),
            KeyboardKey::KeyAdd => str::to_string("Add"),
            KeyboardKey::KeyNumpad1 => str::to_string("Numpad1"),
            KeyboardKey::KeyNumpad2 => str::to_string("Numpad2"),
            KeyboardKey::KeyNumpad3 => str::to_string("Numpad3"),
            KeyboardKey::KeyNumpad0 => str::to_string("Numpad0"),
            KeyboardKey::KeyF11 => str::to_string("F11"),
            KeyboardKey::KeyF12 => str::to_string("F12"),
            KeyboardKey::KeyF13 => str::to_string("F13"),
            KeyboardKey::KeyF14 => str::to_string("F14"),
            KeyboardKey::KeyF15 => str::to_string("F15"),
            KeyboardKey::KeyF16 => str::to_string("F16"),
            KeyboardKey::KeyF17 => str::to_string("F17"),
            KeyboardKey::KeyF18 => str::to_string("F18"),
            KeyboardKey::KeyF19 => str::to_string("F19"),
            KeyboardKey::KeyNumpadEquals => str::to_string("NumpadEquals"),
            KeyboardKey::KeyNumpadEnter => str::to_string("NumpadEnter"),
            KeyboardKey::KeyRControl => str::to_string("RControl"),
            KeyboardKey::KeyNumpadComma => str::to_string("NumpadComma"),
            KeyboardKey::KeyDivide => str::to_string("Divide"),
            KeyboardKey::KeyPause => str::to_string("Pause"),
            KeyboardKey::KeyHome => str::to_string("Home"),
            KeyboardKey::KeyUp => str::to_string("Up"),
            KeyboardKey::KeyLeft => str::to_string("Left"),
            KeyboardKey::KeyRight => str::to_string("Right"),
            KeyboardKey::KeyEnd => str::to_string("End"),
            KeyboardKey::KeyDown => str::to_string("Down"),
            KeyboardKey::KeyNext => str::to_string("Next"),
            KeyboardKey::KeyInsert => str::to_string("Insert"),
            KeyboardKey::KeyDelete => str::to_string("Delete"),
        };
        f.write_str(&name)
    }
}
