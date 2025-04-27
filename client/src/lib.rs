extern crate ctor;
extern crate jni;
mod client;
mod mapping;
mod module;

use crate::client::DarkClient;
use crate::module::FlyModule;
use ctor::*;
use log::{error, info, LevelFilter};
use simplelog::{Config, WriteLogger};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;
use crate::client::keyboard::{start_keyboard_handler};
use crate::mapping::client::minecraft::Minecraft;

static mut TICK_THREAD: Option<thread::JoinHandle<()>> = None;

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
#[ctor]
fn load() {
    // Inizializza il logger con configurazione per il file
    WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
        File::create("dark_client.log").unwrap(),
    ).unwrap();

    thread::spawn(|| {
        info!("Starting DarkClient...");
        let minecraft = Minecraft::instance();

        register_modules(minecraft);

        start_keyboard_handler();

        // Tick thread
        unsafe {
            TICK_THREAD = Some(thread::spawn(move || {
                let client = DarkClient::instance();
                loop {
                    // Aspetta il tick di Minecraft
                    thread::sleep(Duration::from_millis(50)); // 20 ticks al secondo
                    client.tick();
                }
            }));
        }

        info!("Player position: {:?}", minecraft.player.entity.get_position());
    });

    open_connection();
}

#[no_mangle]
#[dtor]
fn unload() {
    info!("Unload");
}

fn reload() {
    info!("Reloading...");
    /*unsafe {
        if let Some(handle) = TICK_THREAD.take() {
            handle.join().unwrap();
        }
    }*/
}

fn register_modules(minecraft: &'static Minecraft) {
    let client = DarkClient::instance();

    let fly_module = Arc::new(Mutex::new(FlyModule::new(
        minecraft.player.clone()
    )));

    let register_module = |module: Arc<Mutex<dyn module::Module + Send + Sync>>| {
        client.register_module(module);
    };

    register_module(fly_module);
}

fn open_connection() {
    thread::spawn(|| {
        let addr = "127.0.0.1:7878";
        let listener = match TcpListener::bind(addr) {
            Ok(listener) => {
                info!("Listening on {}", addr);
                listener
            },
            Err(e) => {
                error!("Failed to bind to {}: {}", addr, e);
                return;
            }
        };

        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let mut reader = BufReader::new(stream);

                    let mut line = String::new();
                    let bytes_read = match reader.read_line(&mut line) {
                        Ok(br) => br,
                        Err(e) => {
                            error!("Failed to read line: {}", e);
                            break;
                        }
                    };

                    if bytes_read == 0 {
                        break;
                    }

                    let command = line.trim();
                    match command {
                        "reload" => {
                            reload();
                        }
                        other => {
                            error!("Unknown command: {}", other);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    });
}