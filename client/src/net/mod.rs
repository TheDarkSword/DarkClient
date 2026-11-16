//! Netty pipeline injection — the packet layer.
//!
//! Defines `DarkChannelHandler` (a thin Netty bridge, bytecode embedded from
//! `client/java/DarkChannelHandler.class`) into Minecraft's class loader, binds
//! its native methods to Rust, and inserts an instance into the live server
//! connection's Netty pipeline.
//!
//! Every packet then flows through [`dispatch`]: it is wrapped into a
//! [`packet::Packet`] value-snapshot (only for the types a module handles),
//! offered to every enabled module's `handle_packet`, and — if a module
//! changed it — rebuilt into a fresh JVM object that replaces the original.
//!
//! Only defining the handler class and binding its natives is raw JNI.

pub mod packet;

use crate::error::{Error, Result};
use crate::net::packet::{Packet, PacketAction};
use crate::state::{env, minecraft};
use jni::objects::{GlobalRef, JClass, JObject, JValue};
use jni::sys::jobject;
use jni::{JNIEnv, NativeMethod};
use mc_bindings::{ChannelPipeline, Connection, Object};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

/// Bytecode of the `DarkChannelHandler` Netty bridge.
const HANDLER_BYTECODE: &[u8] = include_bytes!("../../java/DarkChannelHandler.class");
/// Binary name of the handler class.
const HANDLER_CLASS: &str = "DarkChannelHandler";
/// Name of our entry in Minecraft's Netty pipeline.
const PIPELINE_NAME: &str = "dark_handler";

/// Minecraft's own pipeline entry (`Connection`). Our handler must sit *before*
/// it: inbound packets flow head→tail, so to see — and rewrite — a packet
/// before Minecraft processes it, we have to be upstream of this handler.
const MC_PACKET_HANDLER: &str = "packet_handler";

/// How many times defining the handler class may fail before giving up — a
/// genuine, repeatable failure should not spam the log forever.
const MAX_DEFINE_ATTEMPTS: u32 = 20;

struct NetState {
    /// The `DarkChannelHandler` class, once defined and bound.
    handler_class: Option<GlobalRef>,
    /// The `Connection` our handler is currently installed on.
    installed_on: Option<Connection>,
    /// Consecutive class-definition failures — capped by `MAX_DEFINE_ATTEMPTS`.
    define_attempts: u32,
}

static STATE: Mutex<NetState> = Mutex::new(NetState {
    handler_class: None,
    installed_on: None,
    define_attempts: 0,
});

/// Polls the live connection and makes sure our handler sits on its pipeline.
/// Cheap and idempotent — meant to be called once per game tick.
pub fn ensure_installed() {
    if let Err(error) = ensure_installed_inner() {
        log::debug!("net: install pass failed: {error}");
    }
    clear_pending_exception();
}

/// Drops any stray pending JNI exception. This code runs inside Minecraft's
/// `glfwSwapBuffers` call — a leaked exception would crash the game the moment
/// control returns to its Java frame.
fn clear_pending_exception() {
    if let Ok(env) = env() {
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            log::warn!("net: cleared a stray JNI exception");
        }
    }
}

/// Defines and binds the handler class the first time; `None` while that keeps failing.
fn handler_class() -> Option<GlobalRef> {
    let mut state = STATE.lock().unwrap();
    if state.handler_class.is_none() && state.define_attempts < MAX_DEFINE_ATTEMPTS {
        match define_handler() {
            Ok(class) => {
                log::info!("net: DarkChannelHandler defined and bound");
                state.handler_class = Some(class);
                state.define_attempts = 0;
            }
            Err(error) => {
                state.define_attempts += 1;
                log::warn!(
                    "net: handler definition failed (attempt {}/{}): {error}",
                    state.define_attempts,
                    MAX_DEFINE_ATTEMPTS
                );
            }
        }
    }
    state.handler_class.clone()
}

fn ensure_installed_inner() -> Result<()> {
    let Some(handler_class) = handler_class() else {
        return Ok(());
    };

    // Find the live server connection.
    let Some(connection) = current_connection()? else {
        STATE.lock().unwrap().installed_on = None;
        return Ok(());
    };

    // Already installed on this exact connection?
    {
        let state = STATE.lock().unwrap();
        if let Some(previous) = &state.installed_on {
            if previous.is_same(&connection) {
                return Ok(());
            }
        }
    }

    install_on(&connection, &handler_class)?;
    // `replace` returns the previous connection, if any — a `Some` means this
    // is a *re*-install onto a fresh connection (a server change / reconnect),
    // not the first install, so the log says which.
    let reinstalled = STATE
        .lock()
        .unwrap()
        .installed_on
        .replace(connection)
        .is_some();
    if reinstalled {
        log::info!(
            "net: handler re-installed — the connection changed (server switch / reconnect)"
        );
    } else {
        log::info!("net: handler installed on the connection pipeline");
    }
    Ok(())
}

/// Makes the `DarkChannelHandler` class available and binds its native methods
/// to this library's functions.
///
/// On the first injection the class is `DefineClass`'d into the game class
/// loader. On a **hot-reload** it is already defined there — a class name can
/// be defined only once per loader, and a second `DefineClass` throws a
/// `LinkageError` — so the existing class is reused instead. Either way the
/// native methods are (re)bound: on reload the previous binding points into the
/// now-unloaded old library and *must* be replaced.
fn define_handler() -> Result<GlobalRef> {
    let mut env = env()?;
    let loader = mc_bindings::runtime()?
        .game_class_loader()
        .ok_or(mc_bindings::Error::NoClassLoader)?;

    let class = match load_existing_handler(&mut env, &loader) {
        Some(existing) => existing,
        None => env
            .define_class(HANDLER_CLASS, loader.as_obj(), HANDLER_BYTECODE)
            .map_err(|error| Error::jni(&mut env, "DefineClass", error))?,
    };

    let methods = [
        NativeMethod {
            name: "onOutbound".into(),
            sig: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            fn_ptr: dark_on_outbound as *mut c_void,
        },
        NativeMethod {
            name: "onInbound".into(),
            sig: "(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            fn_ptr: dark_on_inbound as *mut c_void,
        },
    ];
    env.register_native_methods(&class, &methods)
        .map_err(|error| Error::jni(&mut env, "RegisterNatives", error))?;

    let global = env
        .new_global_ref(&class)
        .map_err(|error| Error::jni(&mut env, "NewGlobalRef", error));
    let _ = env.delete_local_ref(class);
    global
}

/// Returns the `DarkChannelHandler` class if a previous injection already
/// defined it in `loader` (the hot-reload case), otherwise `None`.
fn load_existing_handler<'a>(env: &mut JNIEnv<'a>, loader: &GlobalRef) -> Option<JClass<'a>> {
    let Ok(name) = env.new_string(HANDLER_CLASS) else {
        return None;
    };
    let result = env.call_method(
        loader.as_obj(),
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&name)],
    );
    match result.and_then(|value| value.l()) {
        Ok(class) if !class.is_null() => Some(JClass::from(class)),
        _ => {
            // Not defined yet — `loadClass` threw `ClassNotFoundException`.
            let _ = env.exception_clear();
            None
        }
    }
}

fn current_connection() -> Result<Option<Connection>> {
    Ok(match minecraft().get_connection()? {
        Some(listener) => Some(listener.get_connection()?),
        None => None,
    })
}

/// Goes before Minecraft's `packet_handler`, so we see every packet first.
fn install_on(connection: &Connection, handler_class: &GlobalRef) -> Result<()> {
    let pipeline = pipeline_of(connection)?;
    if pipeline.get(PIPELINE_NAME)?.is_some() {
        return Ok(());
    }
    let handler = new_handler(handler_class)?;
    pipeline.add_before(MC_PACKET_HANDLER, PIPELINE_NAME, &handler)?;
    Ok(())
}

fn new_handler(class: &GlobalRef) -> Result<Object> {
    let mut env = env()?;
    let local = env
        .new_object(class, "()V", &[])
        .map_err(|error| Error::jni(&mut env, "new DarkChannelHandler()", error))?;
    let handler = Object::from_local(&local);
    let _ = env.delete_local_ref(local);
    handler?.ok_or(Error::Null("new DarkChannelHandler()"))
}

/// Must run before unload: a handler left on the pipeline would call into
/// unmapped code.
pub fn teardown() {
    let (connection, class) = {
        let mut state = STATE.lock().unwrap();
        // A fresh injection gets a fresh definition-retry budget.
        state.define_attempts = 0;
        (state.installed_on.take(), state.handler_class.take())
    };

    if let Some(connection) = connection {
        let removed = pipeline_of(&connection).and_then(|pipeline| {
            if pipeline.get(PIPELINE_NAME)?.is_some() {
                pipeline.remove(PIPELINE_NAME)?;
            }
            Ok(())
        });
        if let Err(error) = removed {
            log::debug!("net: pipeline cleanup failed: {error}");
        }
    }

    if let Some(class) = class {
        if let Ok(mut env) = env() {
            if let Ok(local) = env.new_local_ref(class.as_obj()) {
                let _ = env.unregister_native_methods(JClass::from(local));
            }
        }
    }
    clear_pending_exception();
}

fn pipeline_of(connection: &Connection) -> Result<ChannelPipeline> {
    Ok(connection.channel()?.pipeline()?)
}

// --- packet dispatch -------------------------------------------------------

/// What the dispatch decided the Netty callback should forward.
enum Dispatch {
    /// Forward the original packet object, untouched.
    Forward,
    /// Forward this freshly built object in place of the original.
    Replace(jobject),
    /// Drop the packet — the callback returns `null` so Netty discards it
    /// (the packet is never sent, outbound, nor delivered, inbound).
    Drop,
}

/// `DarkChannelHandler.onOutbound` — dispatches an outbound packet.
unsafe extern "system" fn dark_on_outbound(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    packet: jobject,
) -> jobject {
    forward(env, packet, false)
}

/// `DarkChannelHandler.onInbound` — dispatches an inbound packet.
unsafe extern "system" fn dark_on_inbound(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    packet: jobject,
) -> jobject {
    forward(env, packet, true)
}

/// What the Netty callback hands back: the packet, its replacement, or `null` to drop it.
unsafe fn forward(env: *mut jni::sys::JNIEnv, packet: jobject, inbound: bool) -> jobject {
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        dispatch(env, packet, inbound)
    })) {
        Ok(Ok(Dispatch::Replace(replacement))) => replacement,
        Ok(Ok(Dispatch::Drop)) => std::ptr::null_mut(),
        Ok(Ok(Dispatch::Forward)) => packet,
        Ok(Err(error)) => {
            let what = if inbound {
                "net: inbound packet forwarded untouched"
            } else {
                "net: outbound packet forwarded untouched"
            };
            crate::error::warn_changed(what, &error);
            packet
        }
        // The panic hook has logged it already.
        Err(_) => packet,
    }
}

/// Wraps a packet for the modules, lets each enabled module's `handle_packet`
/// modify or cancel it, and rebuilds the JVM object if it changed. Every packet
/// type no module handles — and every error — yields [`Dispatch::Forward`], so
/// the connection is never disrupted by this layer. `inbound` selects which
/// `Packet` variants to probe.
unsafe fn dispatch(env: *mut jni::sys::JNIEnv, packet: jobject, inbound: bool) -> Result<Dispatch> {
    if packet.is_null() {
        return Ok(Dispatch::Forward);
    }
    let mut env = unsafe { JNIEnv::from_raw(env) }.map_err(|error| Error::Jni {
        call: "JNIEnv::from_raw",
        reason: error.to_string(),
    })?;
    let packet_obj = unsafe { JObject::from_raw(packet) };

    let built = if inbound {
        Packet::from_inbound(&packet_obj)?
    } else {
        Packet::from_outbound(&packet_obj)?
    };
    let Some(mut wrapped) = built else {
        return Ok(Dispatch::Forward);
    };

    let original = wrapped.clone();
    match crate::state::client().modules.handle_packet(&mut wrapped) {
        PacketAction::Cancel => Ok(Dispatch::Drop),
        PacketAction::Forward if wrapped != original => {
            let replacement = wrapped.to_object()?;
            let local = env
                .new_local_ref(replacement.as_obj())
                .map_err(|error| Error::jni(&mut env, "NewLocalRef", error))?;
            Ok(Dispatch::Replace(local.into_raw()))
        }
        PacketAction::Forward => Ok(Dispatch::Forward),
    }
}
