#![cfg_attr(debug_assertions, allow(dead_code))]

extern crate jni;
mod client;
mod mapping;
mod module;

use crate::client::keyboard::{start_keyboard_handler, stop_keyboard_handler};
use crate::client::DarkClient;
use crate::mapping::client::minecraft::Minecraft;
use crate::module::{FlyModule, ModuleType};
use log::{error, info, LevelFilter};
use simplelog::{Config, WriteLogger};
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

static TICK_THREAD: OnceLock<Mutex<Option<thread::JoinHandle<()>>>> = OnceLock::new();

// Flag to control if the client is running
static RUNNING: AtomicBool = AtomicBool::new(false);

fn tick_thread() -> &'static Mutex<Option<thread::JoinHandle<()>>> {
    TICK_THREAD.get_or_init(|| Mutex::new(None))
}

pub trait LogExpect<T> {
    fn log_expect(self, msg: &str) -> T;
}

impl<T, E: std::fmt::Debug> LogExpect<T> for Result<T, E> {
    fn log_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|e| {
            error!("{}: {:?}", msg, e);
            panic!("{}: {:?}", msg, e);
        })
    }
}

impl<T> LogExpect<T> for Option<T> {
    fn log_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|| {
            error!("{}", msg);
            panic!("{}", msg);
        })
    }
}

#[no_mangle]
pub extern "C" fn initialize_client() {
    // Make sure we can't initialize more than once
    if RUNNING.swap(true, Ordering::SeqCst) {
        info!("Client already initialized");
        return;
    }

    // Initialize the logger
    match WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
        File::create("dark_client.log").unwrap(),
    ) {
        Ok(_) => info!("Logger initialized"),
        Err(e) => eprintln!("Error during logger initialization: {:?}", e),
    }

    thread::spawn(|| {
        info!("Starting DarkClient...");
        let minecraft = Minecraft::instance();

        register_modules(minecraft);

        start_keyboard_handler();

        // Tick thread
        let thread_handle = thread::spawn(move || {
            let client = DarkClient::instance();
            while RUNNING.load(Ordering::SeqCst) {
                // Wait for Minecraft tick
                thread::sleep(Duration::from_millis(50)); // 20 ticks per second
                client.tick();
            }
            info!("Tick thread terminated");
        });

        // Memorize the thread handle in a thread-safe way
        let mut tick_lock = tick_thread().lock().unwrap();
        *tick_lock = Some(thread_handle);

        info!(
            "Player position: {:?}",
            minecraft.player.entity.get_position()
        );
    });
}

// Cleanup function for agent_loader
#[no_mangle]
pub extern "C" fn cleanup_client() {
    info!("Client cleanup in progress...");

    // Set the execution flag to false
    RUNNING.store(false, Ordering::SeqCst);

    // Stop the keyboard handler
    stop_keyboard_handler();

    // Wait for the tick thread to terminate
    let thread_handle = {
        let mut tick_lock = tick_thread().lock().unwrap();
        tick_lock.take()
    };

    if let Some(handle) = thread_handle {
        // Give a short timeout for waiting
        if let Err(e) = handle.join() {
            error!("Error while waiting for tick thread: {:?}", e);
        }
    }

    // Clean up other resources if necessary
    info!("Client cleanup completed");
}

fn register_modules(minecraft: &'static Minecraft) {
    let client = DarkClient::instance();

    let fly_module = Arc::new(Mutex::new(FlyModule::new(minecraft.player.clone())));

    let register_module = |module: Arc<Mutex<ModuleType>>| {
        client.register_module(module);
    };

    register_module(fly_module);
}
