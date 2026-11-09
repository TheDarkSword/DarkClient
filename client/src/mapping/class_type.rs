use std::fmt;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MinecraftClassType {
    Minecraft,
    LocalPlayer,
    Level,
    Player,
    Abilities,
    Entity,
    Vec3,
    Window,
    MultiPlayerGameMode,
    Iterable,
    Iterator,
    Mob,
    Screen,
    Gui,
    KeyboardHandler,
    GameRenderer,
    Camera,
    DeltaTracker,
    GameRenderState,
    LevelRenderState,
    CameraRenderState,
    CameraEntityRenderState,
    OptionsRenderState,
    LivingEntity,
    Component,
    LevelReader,
    LevelChunk,
    BlockEntity,
    ChestBlockEntity,
    EnderChestBlockEntity,
    BarrelBlockEntity,
    ShulkerBoxBlockEntity,
    BlockPos,
    Vec3i,
    Map,
    Object,
    PlayerInfo,
    GameType,
    Options,
    OptionInstance,
    Integer,
    Double,
    InteractionHand,
    Inventory,
    ItemStack,
    Item,
    Items,
    AbstractContainerMenu,
    ContainerInput,
    EquipmentSlot,
    BlockState,
    Direction,
    BlockHitResult,
    ChatScreen,
    PauseScreen,
    AbstractContainerScreen,
    InventoryScreen,
    CraftingScreen,
    CreativeModeInventoryScreen,
    // Network packets — the connection layer (`net/`) reads and rewrites these.
    ServerboundMovePlayerPacket,
    ServerboundMovePlayerPacketPos,
    ServerboundMovePlayerPacketPosRot,
    ServerboundMovePlayerPacketRot,
    ServerboundMovePlayerPacketStatusOnly,
    ClientboundSetEntityMotionPacket,
    ClientboundPlayerInfoUpdatePacket,
}

impl MinecraftClassType {
    pub fn get_name(&self) -> &str {
        match self {
            MinecraftClassType::Minecraft => "net/minecraft/client/Minecraft",
            MinecraftClassType::LocalPlayer => "net/minecraft/client/player/LocalPlayer",
            MinecraftClassType::Level => "net/minecraft/client/multiplayer/ClientLevel",
            MinecraftClassType::Player => "net/minecraft/world/entity/player/Player",
            MinecraftClassType::Abilities => "net/minecraft/world/entity/player/Abilities",
            MinecraftClassType::Entity => "net/minecraft/world/entity/Entity",
            MinecraftClassType::Vec3 => "net/minecraft/world/phys/Vec3",
            MinecraftClassType::Window => "com/mojang/blaze3d/platform/Window",
            MinecraftClassType::MultiPlayerGameMode => {
                "net/minecraft/client/multiplayer/MultiPlayerGameMode"
            }
            MinecraftClassType::Iterable => "java/lang/Iterable",
            MinecraftClassType::Iterator => "java/util/Iterator",
            MinecraftClassType::Mob => "net/minecraft/world/entity/Mob",
            MinecraftClassType::Screen => "net/minecraft/client/gui/screens/Screen",
            MinecraftClassType::Gui => "net/minecraft/client/gui/Gui",
            MinecraftClassType::KeyboardHandler => "net/minecraft/client/KeyboardHandler",
            MinecraftClassType::GameRenderer => "net/minecraft/client/renderer/GameRenderer",
            MinecraftClassType::DeltaTracker => "net/minecraft/client/DeltaTracker",
            MinecraftClassType::GameRenderState => {
                "net/minecraft/client/renderer/state/GameRenderState"
            }
            MinecraftClassType::LevelRenderState => {
                "net/minecraft/client/renderer/state/level/LevelRenderState"
            }
            MinecraftClassType::CameraRenderState => {
                "net/minecraft/client/renderer/state/level/CameraRenderState"
            }
            MinecraftClassType::CameraEntityRenderState => {
                "net/minecraft/client/renderer/state/level/CameraEntityRenderState"
            }
            MinecraftClassType::OptionsRenderState => {
                "net/minecraft/client/renderer/state/OptionsRenderState"
            }
            MinecraftClassType::Camera => "net/minecraft/client/Camera",
            MinecraftClassType::LivingEntity => "net/minecraft/world/entity/LivingEntity",
            MinecraftClassType::Component => "net/minecraft/network/chat/Component",
            MinecraftClassType::LevelReader => "net/minecraft/world/level/LevelReader",
            MinecraftClassType::LevelChunk => "net/minecraft/world/level/chunk/LevelChunk",
            MinecraftClassType::BlockEntity => "net/minecraft/world/level/block/entity/BlockEntity",
            MinecraftClassType::ChestBlockEntity => {
                "net/minecraft/world/level/block/entity/ChestBlockEntity"
            }
            MinecraftClassType::EnderChestBlockEntity => {
                "net/minecraft/world/level/block/entity/EnderChestBlockEntity"
            }
            MinecraftClassType::BarrelBlockEntity => {
                "net/minecraft/world/level/block/entity/BarrelBlockEntity"
            }
            MinecraftClassType::ShulkerBoxBlockEntity => {
                "net/minecraft/world/level/block/entity/ShulkerBoxBlockEntity"
            }
            MinecraftClassType::BlockPos => "net/minecraft/core/BlockPos",
            MinecraftClassType::Vec3i => "net/minecraft/core/Vec3i",
            MinecraftClassType::Map => "java/util/Map",
            MinecraftClassType::Object => "java/lang/Object",
            MinecraftClassType::PlayerInfo => "net/minecraft/client/multiplayer/PlayerInfo",
            MinecraftClassType::GameType => "net/minecraft/world/level/GameType",
            MinecraftClassType::Options => "net/minecraft/client/Options",
            MinecraftClassType::OptionInstance => "net/minecraft/client/OptionInstance",
            MinecraftClassType::Integer => "java/lang/Integer",
            MinecraftClassType::Double => "java/lang/Double",
            MinecraftClassType::InteractionHand => "net/minecraft/world/InteractionHand",
            MinecraftClassType::Inventory => "net/minecraft/world/entity/player/Inventory",
            MinecraftClassType::ItemStack => "net/minecraft/world/item/ItemStack",
            MinecraftClassType::Item => "net/minecraft/world/item/Item",
            MinecraftClassType::Items => "net/minecraft/world/item/Items",
            MinecraftClassType::AbstractContainerMenu => {
                "net/minecraft/world/inventory/AbstractContainerMenu"
            }
            MinecraftClassType::ContainerInput => "net/minecraft/world/inventory/ContainerInput",
            MinecraftClassType::EquipmentSlot => "net/minecraft/world/entity/EquipmentSlot",
            MinecraftClassType::BlockState => "net/minecraft/world/level/block/state/BlockState",
            MinecraftClassType::Direction => "net/minecraft/core/Direction",
            MinecraftClassType::BlockHitResult => "net/minecraft/world/phys/BlockHitResult",
            MinecraftClassType::ChatScreen => "net/minecraft/client/gui/screens/ChatScreen",
            MinecraftClassType::PauseScreen => "net/minecraft/client/gui/screens/PauseScreen",
            MinecraftClassType::AbstractContainerScreen => {
                "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen"
            }
            MinecraftClassType::InventoryScreen => {
                "net/minecraft/client/gui/screens/inventory/InventoryScreen"
            }
            MinecraftClassType::CraftingScreen => {
                "net/minecraft/client/gui/screens/inventory/CraftingScreen"
            }
            MinecraftClassType::CreativeModeInventoryScreen => {
                "net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen"
            }
            MinecraftClassType::ServerboundMovePlayerPacket => {
                "net/minecraft/network/protocol/game/ServerboundMovePlayerPacket"
            }
            MinecraftClassType::ServerboundMovePlayerPacketPos => {
                "net/minecraft/network/protocol/game/ServerboundMovePlayerPacket$Pos"
            }
            MinecraftClassType::ServerboundMovePlayerPacketPosRot => {
                "net/minecraft/network/protocol/game/ServerboundMovePlayerPacket$PosRot"
            }
            MinecraftClassType::ServerboundMovePlayerPacketRot => {
                "net/minecraft/network/protocol/game/ServerboundMovePlayerPacket$Rot"
            }
            MinecraftClassType::ServerboundMovePlayerPacketStatusOnly => {
                "net/minecraft/network/protocol/game/ServerboundMovePlayerPacket$StatusOnly"
            }
            MinecraftClassType::ClientboundSetEntityMotionPacket => {
                "net/minecraft/network/protocol/game/ClientboundSetEntityMotionPacket"
            }
            MinecraftClassType::ClientboundPlayerInfoUpdatePacket => {
                "net/minecraft/network/protocol/game/ClientboundPlayerInfoUpdatePacket"
            }
        }
    }
}

// Implement Display for better error messages
impl fmt::Display for MinecraftClassType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_name())
    }
}
