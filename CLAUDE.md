# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

DarkClient is a Minecraft (Java Edition) modification framework written in Rust. It injects native libraries into a running Minecraft JVM and drives the game through JNI. It targets **unobfuscated** Minecraft 26.x — vanilla, Fabric and Forge/NeoForge alike — and only the exact version ids in `mc_versions::SUPPORTED_VERSIONS`; anything else (snapshots included) is refused unless the user forces it. Obfuscated Minecraft (≤ 1.21.11) is not supported.

It is a Cargo workspace of seven crates — `mc_versions`, `protocol`, `injector`, `agent_loader`, `client`, `mapping_derive`, `mc_bindgen` — plus an `xtask` helper. `default-members` leaves the tooling (`mc_bindgen`, `xtask`) out of a plain `cargo build` / `cargo check`; `--workspace` includes it.

## Build & Common Commands

```bash
cargo build --release              # build the shipped crates (tooling excluded)
cargo build -p client --release    # build a single crate
cargo check --workspace            # fast check (preferred while iterating)
cargo test --workspace             # all tests (needs a JDK — see Tests below)
cargo test -p client overload      # run tests matching a name
cargo fmt --all
cargo clippy --workspace
cargo xtask e2e                    # manual end-to-end harness (see Tests)
cargo xtask mc-dump --supported    # refresh mc-data/dumps from Mojang's jars (needs curl)
```

- **Always build `--release` for runtime artifacts.** The release profile (`opt-level = "s"`, `lto = true`) is what CI and runtime expect; debug builds also silence `dead_code` warnings via `lib.rs`.
- **Windows requires the nightly toolchain** (see `.github/workflows/build.yml`) and a discoverable `jvm.lib`; `client/build.rs` and `agent_loader/build.rs` locate it via `JAVA_HOME` or `JVM_LIB_DIR`. On Linux `client/build.rs` links `libjvm.so` (located via `java-locator`) so the test executables resolve JNI symbols. JDK 21+ required.
- Running the `injector` needs root (`sudo`) on Linux / Administrator on Windows. `libagent_loader` and `libclient` must sit next to the injector executable or in its working directory.

## Crate Roles

- **`mc_versions/`** — dependency-free: `MinecraftVersion` (parse/order version ids) and `SUPPORTED_VERSIONS` / `is_supported`, shared by injector and client.
- **`protocol/`** — small library shared by `injector`, `agent_loader` and `client`: the localhost socket address (`SOCKET_ADDR`), the `Command` (injector → agent) and `Response` (agent → injector) line formats with `encode`/`decode`, the agent ↔ client C ABI (`client_abi`: symbol names, `StartFn` signature, status codes), and a non-panicking file-logger helper.
- **`injector/`** — standalone binary. A redesigned egui GUI (`gui/`); `--tui` for a crossterm TUI; `--list` / `--inject <pid> [--force]` headless modes. Discovers Minecraft processes — reading the game version from `--fml.mcVersion` (Forge/NeoForge) or the `version.json` of the client jar on the class/module path (vanilla/Fabric); `--version` is only the launcher profile name — refuses a known unsupported version unless forced (GUI checkbox, TUI `f`, `--force`), injects `agent_loader`, then drives the client and shows its reply. UI-agnostic core in `app.rs`; injection orchestration in `inject.rs`; the platform layer (`platform/`) is an `AgentInjector` trait with `linux` / `windows` / `macos` implementations.
- **`agent_loader/`** — `cdylib` injected first. A `#[ctor]` runs on load. Split into focused modules: `logging`, `jvm` (discovery + health monitor), `server` (TCP accept loop), `command` (dispatch), `library` (client lifecycle), `platform` (signal handlers).
- **`client/`** — `cdylib`, the actual mod framework. JNI-driven game interaction, OpenGL overlay, module system.
- **`mapping_derive/`** — `proc-macro` crate. Provides `#[derive(MappedObject)]` for the JVM-object wrappers in `client` (see *Game wrappers* below).
- **`mc_bindgen/`** — tooling only, never linked into the shipped crates: downloads a version's client jar (`fetch`, system `curl`, SHA-1-checked, cached in `target/mc-cache/`), reads class files with `noak` (`classfile`), and reads/writes version dumps (`dump`).
- **`xtask/`** — workspace task runner; `cargo xtask e2e` is the manual Tier-3 test, `cargo xtask mc-dump` writes the version dumps.
- **`mc-data/`** (data, not a crate) — `classes.txt` lists every Minecraft class the client binds; `dumps/<version>.txt` holds, for each supported version, those classes and their supertypes with every non-synthetic field and method (name, descriptor, flags), sorted so the diff between two versions shows what changed.

## Injection & Hot-Reload Flow

This is the core control flow and spans `injector`, `protocol`, `agent_loader`, `client`:

1. `injector` injects `libagent_loader.so`/`.dll` into the JVM process — ptrace (`ptrace-inject`) on Linux, `dll-syringe` on Windows.
2. `agent_loader`'s `#[ctor]` `agent_onload()` starts a TCP server on `protocol::SOCKET_ADDR` (**`127.0.0.1:7878`** — defined once, in `protocol`).
3. `injector` connects and sends a `protocol::Command::Reload { library, config_dir, force }` — the absolute libclient path, the injector's own working directory and the version-check override. The agent loader exports `config_dir` as the `DARK_CONFIG_DIR` env var so the client knows where to keep its config.
4. `agent_loader`'s `library` module copies the library to a uniquely named temp file (avoids file locks), `dlopen`s it, and calls the exported `start_client`. The client builds its `Mapping`, reads the running version and, if it is not supported (and not forced), returns `UNSUPPORTED` without installing anything; the agent then cleans it up and drops it. The agent answers the injector with a `protocol::Response` (`started` / `unsupported` / `failed`) on the same connection.
5. Re-injecting repeats step 3 → `library::reload` cleans up and drops the old library (calling `cleanup_client`) before loading the new one. This is the hot-reload path.

`client` exposes exactly two `#[no_mangle] extern "C"` symbols, typed by `protocol::client_abi`: `start_client` and `cleanup_client`. `start_client` runs `state::init_mapping()` and the version check synchronously, then spawns a thread that calls `state::init_client()`, `register_modules()`, `config::load()` and installs hooks — in that fixed order.

## client/ Internals

**Global state (`state.rs`)**: the client has no `Type::instance()` singletons. Two things live for the whole session, each built once by `state::init()` and reached through a free accessor: the JNI bridge — `mapping()` — and the running game/module state — `client()`, with `minecraft()` a shortcut for `&client().minecraft` and `env()` for a JNI environment. Accessors `expect` the state to exist (using one before `init()` is a programmer error). `init()` builds the `Mapping` first, then the `Client`.

**Windowing & graphics backends**: the old paths stay for older versions; newer ones are added alongside and picked at runtime.

| Minecraft | Window + input | Frame driver |
|---|---|---|
| ≤ 26.1.x | GLFW | GL buffer swap (`glfwSwapBuffers`, else `wglSwapBuffers` / `glXSwapBuffers`) |
| 26.2 | GLFW | GL swap, or Vulkan present ("Prefer Vulkan") |
| 26.3+ | **SDL3** (no GLFW) | `SDL_GL_SwapWindow`, or Vulkan present |

**Rendering & ticking** (`graphic/hook.rs`): `install_hooks` first tries `graphic::vulkan::install` — when `Minecraft.windowSurface()`'s backend is a `VulkanGpuSurface` it swaps LWJGL's `VKCapabilitiesDevice.vkQueuePresentKHR` pointer (re-read on every present) for our own; otherwise it `ilhook`s the GL buffer-swap function from `graphic/platform/` (`frame_hook_targets`, SDL3 first). Either way each frame runs `hook::frame_logic` (`check_tick`, smooth rotations, lazy `input::init`) then paints the overlay: `ui_engine::run_frame` lays out and tessellates egui once, `ui_engine::render_gl` paints it with egui_glow inside a `gl_state::GlStateGuard` — Minecraft's `GlStateManager` caches GL state on the Java side and skips "redundant" calls, so any state the overlay leaves changed corrupts the next frame — and `vulkan/overlay.rs` paints it with `egui-ash-renderer` onto the swapchain image (render pass with `LOAD`, our submit waits on the present's semaphores, the present waits on ours). The Vulkan teardown runs on the render thread at the next present; `uninstall` waits for it. `check_tick` treats any change of the player's tick count as a new tick (a respawned player restarts from zero) and calls `client().modules.tick()`. Tick logic runs on the render thread, not a Minecraft thread. GL entry points resolve through `SDL_GL_GetProcAddress` when SDL3 is loaded (EGL on Wayland), else the system GL library.

**Input** (`graphic/input/`): two backends feed shared logic in `mod.rs` — `glfw.rs` swaps the window's key/mouse/cursor/scroll/char callbacks; `sdl.rs` installs an `SDL_SetEventFilter` (chaining any previous filter) that drops consumed events, toggles `SDL_SetWindowRelativeMouseMode`, and starts SDL text input while the GUI is open. SDL scancodes are translated to GLFW key codes, the numbering `KeyboardKey` and saved keybinds use. **Right Shift** (key `344`) toggles the GUI, **Esc** closes it; while the GUI is open, input events (including the scroll wheel, which is forwarded to egui instead) are consumed rather than passed to Minecraft. Module keybinds toggle modules on key press. Ctrl+C / X / V become egui clipboard events; the clipboard itself goes through Minecraft's `KeyboardHandler` (works on GLFW and SDL alike), read on the render thread, never inside an input callback.

**Module system** (`module/`): implement the `Module` trait (`on_start`/`on_stop`/`on_tick`, all returning `anyhow::Result<()>`; optional `handle_packet` — see *Packet layer* below) plus `ModuleData` accessors. Register new modules in `register_modules()` in `client/src/lib.rs`. Modules carry typed `ModuleSetting`s (Toggle/Slider/Choice/Color). Each module has a stable identity — the `ModuleId` enum — and is registered/looked up by it (never by a name string). The `ModuleRegistry` (`module/registry.rs`) is a `DashMap<ModuleId, _>`; reach it through `client().modules`. `register()` also snapshots each module's factory defaults, which `ModuleRegistry::reset_settings()` (the GUI's "Reset Settings" button) restores. Render modules are configuration holders — `graphic/esp.rs` draws ESP, tracers and nametags from their settings, and `graphic/hud.rs` draws the module list only while the `ArrayList` module is enabled (checked directly, so it also applies at the main menu).

`enabled` is only an *armed* flag (persisted, shown by the GUI toggle) — a module actually **runs** only in a world with a **living** player. `ModuleRegistry::set_active()`, called from `check_tick`, fires `on_start` on every enabled module on each transition into that state and `on_stop` out of it — so modules stand down at the main menu, and also while the player is dead on the respawn screen (a cheat acting then is an instant ban on most servers), then reactivate on respawn. Toggling a module at the menu just flips the flag, with no `on_start` until it next becomes active. `tick()` and `handle_packet` are gated the same way.

**Config persistence** (`config.rs`): each module's keybind, setting values and enabled state — plus the GUI layout (category-panel positions and which modules are expanded) — are written to `dark_client_config.json` so they survive a re-injection. The file lives in the **injector's working directory** (passed via `DARK_CONFIG_DIR`) — deliberately *not* in `.minecraft`, to leave no trace in the game directory; it falls back to the process working directory if the variable is unset. `config::save()` runs whenever the user changes something (GUI close, module toggle, "Reset Settings", and before a Panic unload); `config::load()` runs once, right after `register_modules()`, and re-applies the saved state — re-enabling modules that were left on.

**Game wrappers** (`mapping/client/`, `mapping/entity/`): `Minecraft` holds only what exists from the main menu onward — the `getInstance()` handle and the `Window`. The world-scoped objects are lazy accessors — `player()`, `world()`, `game_mode()` return `Result<Option<_>>`, where `Ok(None)` means "not in a world". This lets the client be injected from the main menu; world-dependent modules early-return when `None`. Each wrapper of a live JVM object derives `MappedObject` (`#[derive(MappedObject)]`, from the `mapping_derive` crate), which gives it `jni_ref()`, `class_type()`, and the `call_method`/`get_field`/`set_field`/`instance_of`/`is_same`/`equals` helpers. Immutable value types (`Vec3`, `BlockPos`, …) are instead read once into plain Rust fields — a value-snapshot, no JNI handle retained.

The entity wrappers (`mapping/entity/`) mirror Minecraft's `Entity → LivingEntity → Player` class hierarchy with **traits**, since Rust has no inheritance: `PlayerRef: LivingEntityRef: EntityRef`, each a trait of provided methods. A wrapper struct (`Entity`, `LivingEntity`, `Mob`, `Player`, `LocalPlayer`) is `{ jni_ref }` plus empty `impl`s of the trait for its level and every level below — so a `Player` value can call `Entity`, `LivingEntity` and `Player` methods directly (the methods reflect against the wrapper's own class, and reflection resolves inherited methods). Static dispatch — zero-cost.

**Mapping system** (`mapping/`): bridges the Mojmap names the code uses — `MinecraftClassType` and member string literals — to the running JVM. Minecraft 26.x is unobfuscated, so names are the real ones; method signatures — still required by JNI — are discovered lazily via `java.lang.Class` reflection in `reflect.rs` and cached. `Mapping::new()` fails on an obfuscated build (the Mojmap `Minecraft` class does not resolve).

Mojang still renames symbols between unobfuscated releases, so `Mapping` reads the running version from `SharedConstants.getCurrentVersion().id()` through the game class loader (identical on vanilla, Fabric and Forge): `version_id()` is the exact id the supported-version check compares, `get_version()` the parsed one for gates (`26.4-snapshot-7` → 26.4.0; unparsable → `MinecraftVersion::LATEST`). Code always uses the **latest** names; `renames.rs` maps them back for older versions. The registry is keyed by the class of the wrapper a call is made on, so members inherited through the entity traits, and structural moves (26.2 moved the open screen to `Minecraft.gui.screen()`), use an inline `mapping().get_version() >= MinecraftVersion::new(..)` gate instead.

Reflected classes and resolved class handles live in `DashMap`s (`classes`, `class_handles`). `Mapping` owns its `JavaVM` handle and wraps all JNI calls (`call_method`, `call_static_method`, `get_field`, `set_field`, …); `class.rs` does overload resolution by scoring argument-type compatibility against JNI signatures.

**Mod loaders (`mapping/loader.rs`)**: Fabric and Forge/NeoForge run the game in an isolated class loader, so `find_class` from a native thread resolves a dead duplicate of `Minecraft` whose static `instance` is null. `Mapping::new()` calls `loader::discover_game_loader` first — it scans every live thread's context class loader and keeps the one whose `Minecraft.getInstance()` is non-null. That loader is stored in `class_loader` so every later lookup goes through `ClassLoader.loadClass`. This works loader-agnostically for vanilla, Fabric and Forge/NeoForge.

**Packet layer** (`net/`): intercepts the Minecraft↔server connection — used by modules that must read or rewrite packets (NoFall, Velocity). A thin Netty bridge handler, `DarkChannelHandler` (Java source + committed `.class` in `client/java/`, bytecode embedded via `include_bytes!`), is `DefineClass`'d into the game class loader, its `native` methods bound with `RegisterNatives`, and an instance inserted into the live `Connection`'s Netty pipeline *before* Minecraft's own `packet_handler` (so it sees inbound and outbound packets before the game does) — pure JNI, no JVMTI. A class name can be `DefineClass`'d only once per loader, so on hot-reload the existing class is reused and only its natives are rebound to the new library. `net::ensure_installed` (polled each tick) keeps it installed; `net::teardown` removes it before unload. The handler calls back into `net::dispatch`, which wraps the JVM packet into a `Packet` value-snapshot (`net/packet/`) and offers it to every enabled module's `Module::handle_packet`. A module may **mutate** the snapshot in place — the dispatch then rebuilds a fresh JVM object that replaces the original — or return `PacketAction::Cancel` to **drop** the packet entirely (the callback returns `null`, which the Java handler discards; works for inbound and outbound alike). Minecraft packets are strictly directional, so `Packet::from_outbound` (`Serverbound*`) and `from_inbound` (`Clientbound*`) only probe their own variants; unhandled types return `None` and pass straight through. Packet class names live in the `MinecraftClassType` enum (`mapping/class_type.rs`), not as string literals — each `net/packet/` module exposes a `CLASS_TYPE` constant. All packet/Netty JNI uses explicit descriptors — no reflection. To support a new packet, add its class(es) to `MinecraftClassType`, a value-snapshot module under `net/packet/`, and a `Packet` variant named after the Java class.

**Lifecycle safety**: the global `RUNNING: AtomicBool` gates `on_frame` and the Vulkan present hook. A panic hook installed by `start_client` calls `cleanup_client` so input/render hooks are always uninstalled and GLFW callbacks / the SDL event filter restored, even on panic.

## Supported versions

`mc_versions::SUPPORTED_VERSIONS` lists the exact version ids DarkClient is verified on. Adding one: `cargo xtask mc-dump <id>`, diff its dump against the previous version's to see what changed in the bound classes, handle each change (renames → `renames.rs`, otherwise a version gate), add the id, and test it in game. A class the client starts binding goes into `mc-data/classes.txt` (then re-run `mc-dump --supported`). No mapping files are involved: 26.x ships unobfuscated. The 26.x runtime requires JDK 25.

## Tests

Three tiers (`cargo test --workspace` runs T1 + T2):

- **T1 — unit tests.** Fast, no JVM: `protocol` encode/decode, the injector's process-discovery filter, mapping signature/overload/parse helpers, version-id parsing, SDL scancode translation, pasted-color parsing, ESP projection and view-bobbing math, the dump text format and class-file extraction (a `javac`-compiled fixture; skipped without a JDK). An ignored test parses every class of a real jar: `MC_JAR=<client.jar> cargo test -p mc_bindgen -- --ignored`.
- **T2 — JVM integration** (`client/src/mapping/jvm_test.rs`). Boots an in-process JVM via the `jni` `invocation` feature, with a small Java fixture (`client/tests/java/`, compiled by `javac` at test time) standing in for the game classes. Exercises the reflected mapping path and the `state::init()` menu↔in-world transition. Needs a JDK.
- **T3 — end-to-end** (`cargo xtask e2e`). Builds the workspace, discovers a running Minecraft and injects into it; the overlay check is manual. Needs a running game and root; not run in CI.

## Logs

All three log files — and the client config — are written to the **injector's
working directory** (the agent loader and client receive it via the
`DARK_CONFIG_DIR` env var), so nothing is left in `.minecraft`:

- `injector` → `app.log`
- `agent_loader` → `agent_loader.log` (set up lazily, on the first command, once the directory is known)
- `client` → `dark_client.log`
