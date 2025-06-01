# 🎮 DarkClient - Minecraft Injection Client

![Rust](https://img.shields.io/badge/Rust-1.87.0-orange.svg)
![License](https://img.shields.io/badge/License-GNU%20GPL-blue)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)

A Minecraft hacked client built in Rust, utilizing JNI (Java Native Interface) for seamless integration with Minecraft's Java runtime. DarkClient provides a robust architecture for developing game modifications through dynamic library injection.

### Minecraft Version Mappings: 1.21.5

## 🚀 Features

- **🔧 Dynamic Library Injection**: Hot-swappable module system without requiring game restarts
- **🎨 Cross-Platform GUI**: Beautiful injector interface built with egui
- **⌨️ Real-time Input Handling**: Advanced keyboard event processing for module toggling
- **🗺️ Smart Mapping System**: Automatic obfuscation handling through JSON-based mappings
- **🔄 Module Architecture**: Extensible module system for easy feature development
- **📊 Comprehensive Logging**: Detailed logging system for debugging and monitoring
- **🔒 Thread-Safe Design**: Robust multi-threaded architecture with proper synchronization

## 🏗️ Architecture

The project is organized into three main components:

### 1. **Injector** (`injector/`)
A user-friendly GUI application that handles:
- Process detection (finding Minecraft instances)
- Library injection into target processes
- Status monitoring and error reporting

### 2. **Agent Loader** (`agent_loader/`)
A JVMTI agent that provides:
- Dynamic library loading capabilities
- TCP command server for hot-reloading
- Process lifecycle management
- Cross-platform injection support

### 3. **Client Library** (`client/`)
The core modification framework featuring:
- JNI integration with Minecraft's runtime
- Module system for game modifications
- Mapping system for obfuscation handling
- Input processing and event management

## 📋 Prerequisites

- **Rust 1.87.0+** with Cargo package manager
- **Java Development Kit (JDK) 21+**
- **Minecraft Java Edition**

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

### 3. Prepare Mappings
The framework uses obfuscation mappings to interact with Minecraft:

#### Convert Mojang mappings using the included Python script
```python
python conversion.py
```
#### Place the resulting mappings.json in the project root


## 🎮 Usage

### Quick Start

1. **Launch the Injector**:
   ```bash
   ./target/release/injector
   ```

2. **Start Minecraft** and load into a world

3. **In the Injector GUI**:
  - Click "Find" to detect the Minecraft process
  - Click "Inject" to load the modification framework

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
    
    fn on_start(&self) {
        // Called when module is enabled
    }
    
    fn on_stop(&self) {
        // Called when module is disabled
    }
    
    fn on_tick(&self) {
        // Called every game tick while enabled
    }
}
```
```text
DarkClient/
├── 📁 client/               # Core modification library
│   ├── 📁 src/
│   │   ├── 📄 lib.rs        # Main library entry point
│   │   ├── 📄 client.rs     # DarkClient core & JVM integration
│   │   ├── 📁 mapping/      # Minecraft mapping system
│   │   └── 📁 module/       # Module framework
├── 📁 injector/             # GUI injection tool
│   └── 📁 src/
│       └── 📄 main.rs       # Injector application
├── 📁 agent_loader/         # JVMTI agent for dynamic loading
├── 📄 mappings.json         # Minecraft obfuscation mappings
├── 📄 conversion.py         # Mapping conversion utility
└── 📄 Cargo.toml           # Workspace configuration
```

## 🔧 Configuration
### Logging
Logs are written to:
- - Injector application logs `app.log` is located where injector is executed
- - Client library logs `dark_client.log` is located in .minecraft

### Network Settings
The agent loader uses TCP port `7878` for communication. This can be modified in : `platform/mod.rs`
```rust
pub const SOCKET_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7878);
```

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
