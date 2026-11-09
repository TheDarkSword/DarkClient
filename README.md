# 🎮 DarkClient - Minecraft Injection Client

![Rust](https://img.shields.io/badge/Rust-1.95.0-orange.svg)
![Minecraft](https://img.shields.io/badge/Minecraft-26.1.2-green.svg)
![License](https://img.shields.io/badge/License-GNU%20GPL-blue)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)

A Minecraft hacked client built in Rust, using JNI (Java Native Interface) for seamless integration with Minecraft's Java runtime. DarkClient provides a robust architecture for developing game modifications through dynamic library injection.

### Supported Minecraft Versions

**26.1, 26.1.1, 26.1.2, 26.2 and 26.3** — the unobfuscated releases, with both the OpenGL and the Vulkan renderer. Names are resolved directly and method signatures via runtime JNI reflection; no mapping files are needed.

Any other version — snapshots included — is refused: the injector checks the game version before injecting, and the client checks it again from inside the game. For testing on an unlisted version, tick **Ignore version check** in the injector (`f` in the TUI, `--force` with `--inject`). Obfuscated Minecraft (1.21.11 and older) is not supported.

### Supported Mod Loaders

DarkClient runs on **vanilla Minecraft**, **Fabric**, and **Forge/NeoForge**. It auto-discovers the game's class loader (`KnotClassLoader` for Fabric, `TransformingClassLoader` for Forge/NeoForge), so injection is loader-agnostic.


## 🖼️ Preview

![DarkClient GUI](assets/darkclient_gui.png)

## 🚀 Features

- **🔧 Dynamic Library Injection**: Hot-swappable module system without requiring game restarts
- **🎨 Cross-Platform GUI**: Beautiful injector interface built with egui
- **🖥️ In-Game ClickGUI**: egui overlay with draggable category panels, per-module settings, scrollable lists and rebindable keys
- **⌨️ Real-time Input Handling**: Advanced keyboard/mouse event processing for module toggling
- **🗺️ Mapping System**: Mojmap names resolved at runtime through JNI reflection, with per-version renames handled in one place
- **📡 Packet Layer**: Netty-pipeline interception (pure JNI, no JVMTI) for packet-level modules — powers NoFall and Velocity / Anti-Knockback
- **🔄 Module Architecture**: Extensible module system for easy feature development
- **💾 Persistent Config**: keybinds, settings, enabled modules and GUI layout saved across injections
- **🧩 Mod Loader Support**: Works with vanilla Minecraft, Fabric, and Forge/NeoForge
- **📊 Comprehensive Logging**: Detailed logging system for debugging and monitoring
- **🔒 Thread-Safe Design**: Robust multi-threaded architecture with proper synchronization

## 🏗️ Architecture

A Cargo workspace of five crates plus an `xtask` helper:

### **Protocol** (`protocol/`)
The shared contract between the injector and the agent: the localhost
socket address and the typed command set, defined once so the two ends
cannot drift apart.

### **Injector** (`injector/`)
The injection tool — a redesigned egui GUI, a `--tui` terminal mode and
`--list` / `--inject <pid>` headless modes — that handles:
- Process detection (finding Minecraft instances)
- Library injection into target processes (per-platform, behind a trait)
- Status monitoring and error reporting

### **Agent Loader** (`agent_loader/`)
A `cdylib` injected into the JVM. On load it provides:
- Dynamic library loading and hot-reloading of the client
- A TCP command server
- A JVM health monitor and clean process lifecycle handling

### **Client Library** (`client/`)
The core modification framework featuring:
- JNI integration with Minecraft's runtime
- Module system for game modifications (combat, movement, render)
- Mapping system resolving Minecraft classes and members at runtime
- An in-game ClickGUI overlay, input processing and event management
- A packet layer (`net/`) that injects a handler into Minecraft's Netty pipeline
- Persistent configuration of keybinds, settings and GUI layout

### **Mapping Derive** (`mapping_derive/`)
A `proc-macro` crate providing `#[derive(MappedObject)]` for the JVM-object
wrappers in the client — generates the JNI helper methods so each game
wrapper stays a thin, safe handle.

## 📋 Prerequisites

- **Rust 1.95.0+** with Cargo package manager
- **Java Development Kit (JDK) 21+**
- **Minecraft Java Edition**

## ⬇️ Download

If you prefer precompiled binaries instead of building from source:

1. Go to the **Actions** tab on GitHub.
2. Open the latest workflow run.
3. Scroll to the bottom of the page to find the **Artifacts** section.
4. Download the compiled binaries for your platform (**Linux** or **Windows**).

This allows you to get up and running without waiting for compilation.

## 🛠️ Installation & Setup

### 1. Clone the Repository
```bash
bash git clone https://github.com/TheDarkSword/DarkClient
cd darkclient
```

### 2. Build the Project
```bash
cargo build --release
```


## 🎮 Usage

### Quick Start

1. **Launch the Injector**:
   ```bash
   cd target/release
   ./injector
   ```
> [!WARNING]
> `libagent_loader` and `libclient` **must** be in the **same directory** where you run the injector.

2. **Start Minecraft** — you can inject from the main menu; modules stay
   idle until you load a world

3. **In the Injector GUI**:
- Click "Scan" to detect the Minecraft process
- Select it and click "Inject" to load the modification framework

4. **Use Modules**:
- Modules can be toggled using their assigned keybinds
- Check the log files for module status and debugging info

### Module Development

Create new modules by implementing the `Module` trait:

```rust
use crate::module::{Module, ModuleData};

pub struct CustomModule {
   data: ModuleData,
   // Your module-specific fields
}

impl Module for CustomModule {
   fn get_module_data(&self) -> &ModuleData {
      &self.data
   }

   fn get_module_data_mut(&mut self) -> &mut ModuleData {
      &mut self.data
   }

   fn on_start(&self) -> anyhow::Result<()> {
      // Called when the module is enabled.
      Ok(())
   }

   fn on_stop(&self) -> anyhow::Result<()> {
      // Called when the module is disabled.
      Ok(())
   }

   fn on_tick(&self) -> anyhow::Result<()> {
      // Called every game tick while enabled.
      Ok(())
   }
}
```

Register it in `register_modules()` in `client/src/lib.rs`. Modules that need
to read or rewrite network packets can also implement the optional
`handle_packet` method — see NoFall and Velocity for examples.
```text
DarkClient/
├── 📁 protocol/             # Shared injector ⇆ agent IPC contract
├── 📁 injector/             # Injection tool (GUI / TUI / headless CLI)
├── 📁 agent_loader/         # Injected cdylib: command server + client lifecycle
├── 📁 client/               # Core modification framework
│   └── 📁 src/
│       ├── 📄 lib.rs        # Entry points (start_client / cleanup_client)
│       ├── 📄 state.rs      # Global client + mapping state
│       ├── 📄 config.rs     # Persistent config (keybinds, settings, GUI layout)
│       ├── 📁 mapping/      # Minecraft mapping system (runtime reflection)
│       ├── 📁 graphic/      # Overlay, hooks, input, platform seam
│       ├── 📁 module/       # Module framework + registry
│       └── 📁 net/          # Netty packet layer (packet structs + dispatch)
├── 📁 mapping_derive/       # proc-macro: #[derive(MappedObject)]
├── 📁 xtask/                # Workspace task runner (cargo xtask e2e)
├── 📁 mc_versions/          # Supported versions + version parsing
├── 📁 mc_bindgen/           # Tooling: version dumps from the game jars
├── 📁 mc-data/              # Bound class list + per-version dumps
└── 📄 Cargo.toml            # Workspace configuration
```

## 🔧 Configuration
### Files
DarkClient writes nothing into `.minecraft` — all of its files live in the
directory the **injector** is started from:
- `app.log` — injector log
- `agent_loader.log` — agent loader log
- `dark_client.log` — client log
- `dark_client_config.json` — persisted keybinds, settings, enabled modules and GUI layout

### Network Settings
The injector and agent communicate over TCP `127.0.0.1:7878`, defined once in the `protocol` crate:
```rust
// protocol/src/lib.rs
pub const SOCKET_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878);
```

## 🩹 Patched Dependencies

DarkClient vendors a **patched copy of [`ilhook`](https://crates.io/crates/ilhook)**
(the inline-hook crate behind the frame hook) under `vendor/ilhook`, wired in through
`[patch.crates-io]`. Upstream `ilhook` 2.3.0 has a Linux bug: its hook trampoline is a
heap allocation that can straddle a page boundary, yet only one page is marked
executable — so roughly one inject in four ended in a `SIGSEGV` in native code.

> 📄 Full write-up — causes, crash-log evidence and the fix:
> [`docs/vendored-ilhook-fix.md`](docs/vendored-ilhook-fix.md)

## 🤝 Contributing
1. **Fork** the repository
2. **Create** a feature branch (`git checkout -b feature/amazing-module`)
3. **Commit** your changes (`git commit -am 'Add amazing module'`)
4. **Push** to the branch (`git push origin feature/amazing-module`)
5. **Create** a Pull Request

### Development Guidelines
- Follow Rust best practices and use `cargo fmt`
- Add comprehensive documentation for new modules
- Include proper error handling and logging

## ⚠️ Legal Notice
This project is intended for educational and research purposes. Users are responsible for complying with:
- Minecraft's Terms of Service
- Mojang's Commercial Usage Guidelines
- Local laws and regulations regarding game modifications

## 📄 License
This project is licensed under the GNU GPL License - see the [LICENSE](LICENSE) file for details.
