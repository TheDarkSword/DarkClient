//! JVM discovery and health monitoring.

use std::thread;
use std::time::Duration;

use jni::sys::{jsize, JNI_GetCreatedJavaVMs, JNI_OK};
use jni::JavaVM;
use log::info;

use crate::is_running;

/// How often to poll for / health-check the JVM.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Consecutive failed health checks before the JVM is declared dead.
const MAX_FAILURES: u32 = 3;

/// Spawns the JVM health-monitor thread.
pub fn start_monitor() {
    thread::spawn(monitor);
}

/// Waits for the JVM, then polls its health until it disappears or the agent
/// shuts down. A dead JVM triggers [`crate::shutdown`].
fn monitor() {
    info!("jvm monitor started");

    let Some(jvm) = wait_for_jvm() else {
        return;
    };
    info!("jvm detected; monitoring health");

    let mut failures = 0u32;
    while is_running() {
        thread::sleep(POLL_INTERVAL);

        if jvm_healthy(&jvm) {
            failures = 0;
            continue;
        }

        failures += 1;
        info!("jvm health check failed ({failures}/{MAX_FAILURES})");
        if failures >= MAX_FAILURES {
            info!("jvm appears to be gone; shutting the agent down");
            crate::shutdown();
            break;
        }
    }

    info!("jvm monitor stopped");
}

/// Blocks until a JVM exists in this process, or the agent shuts down.
fn wait_for_jvm() -> Option<JavaVM> {
    while is_running() {
        if let Some(jvm) = get_jvm() {
            return Some(jvm);
        }
        thread::sleep(POLL_INTERVAL);
    }
    None
}

/// Whether the JVM still responds: a non-null pointer, an attachable thread
/// and a resolvable core class.
fn jvm_healthy(jvm: &JavaVM) -> bool {
    if jvm.get_java_vm_pointer().is_null() {
        return false;
    }
    match jvm.attach_current_thread_as_daemon() {
        Ok(mut env) => env.find_class("java/lang/System").is_ok(),
        Err(_) => false,
    }
}

/// Returns a handle to the JVM running in this process, if there is one.
fn get_jvm() -> Option<JavaVM> {
    let mut raw: *mut jni::sys::JavaVM = std::ptr::null_mut();
    let mut count: jsize = 0;

    // SAFETY: standard JNI invocation-API call; both out-parameters are
    // valid for the duration of the call.
    unsafe {
        if JNI_GetCreatedJavaVMs(&mut raw, 1, &mut count) != JNI_OK || count == 0 {
            return None;
        }
        JavaVM::from_raw(raw).ok()
    }
}
