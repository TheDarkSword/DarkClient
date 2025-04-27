use crate::platform::{AGENT_NAME, LIBRARY_NAME, SOCKET_ADDRESS};
use log::{error, info};
use proc_maps::get_process_maps;
use std::io::{Error, Write};
use std::net::{TcpStream};
use std::{path, thread};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn inject(pid: u32) -> Result<(), Error> {
    // First time: load the agent_loader
    let loader_path = PathBuf::from(format!("{}.so", AGENT_NAME));
    let lib_path = PathBuf::from(format!("{}.so", LIBRARY_NAME));

    // Check if agent_loader is already loaded
    if !find_library(pid, "agent_loader") {
        info!("Loading Agent Loader");

        // Load agent_loader via JVMTI
        match Command::new("jcmd")
            .arg(pid.to_string())
            .arg("JVMTI.agent_load")
            .arg(format!("{:?}", path::absolute(&loader_path)?))
            .output() {
            Ok(output) if output.status.success() => {
                info!("Agent Loader loaded via jcmd: {:?}", loader_path);
            }
            Ok(output) => {
                error!("jcmd failed (stderr): {}", String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => {
                error!("Unable to execute jcmd: {:?}", e);
            }
        }

        // Wait a moment for complete initialization
        thread::sleep(Duration::from_millis(500));
    } else {
        info!("Agent Loader already loaded");
    }

    // Send a reload command to agent_loader
    match TcpStream::connect_timeout(&SOCKET_ADDRESS, Duration::from_secs(5)) {
        Ok(mut stream) => {
            let lib_abs_path = match path::absolute(&lib_path) {
                Ok(p) => p,
                Err(e) => {
                    error!("Unable to get absolute path: {:?}", e);
                    return Err(e);
                }
            };

            info!("Connected to {}. Sending reload command", SOCKET_ADDRESS);

            let lib_abs_path = lib_abs_path.to_string_lossy();
            let lib_abs_path = lib_abs_path.trim_matches(|c| c == '"' || c == '\'');
            // Send the command with the absolute path of the library
            let command = format!("reload {}", lib_abs_path);
            info!("Command: {}", command);

            if let Err(e) = stream.write(command.as_bytes()) {
                error!("Unable to send reload command: {:?}", e);
            }
        },
        Err(e) => {
            error!("Unable to connect to server: {:?}", e);
        }
    }

    Ok(())
}

pub fn find_pid() -> Option<u32> {
    let output = Command::new("ps")
        .arg("ax")
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute `ps ax` command")
        .stdout
        .expect("Failed to capture stdout");

    let output = Command::new("grep")
        .arg("minecraft")
        .stdin(output)
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute `grep minecraft` command")
        .stdout
        .expect("Failed to capture stdout");

    let output = Command::new("grep")
        .arg("java")
        .stdin(output)
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute `grep java` command")
        .stdout
        .expect("Failed to capture stdout");

    let output = Command::new("grep")
        .arg("-v")
        .arg("grep")
        .stdin(output)
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute `grep -v grep` command")
        .stdout
        .expect("Failed to capture stdout");

    let output = Command::new("awk")
        .arg("{print $1}")
        .stdin(output)
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute `awk '{print $1}'` command");


    let result = String::from_utf8(output.stdout).expect("Failed to parse output");

    if !result.is_empty() {
        Some(result.trim().parse::<u32>().expect("Failed to parse PID"))
    } else {
        None
    }
}

fn find_library(pid: u32, lib_name: &str) -> bool {
    let maps = get_process_maps(pid as i32).ok();
    if maps.is_none() {
        return false;
    }
    let maps = maps.unwrap();

    for map in maps {
        if let Some(path) = map.filename() {
            if path.ends_with(format!("{}.so", lib_name)) {
                // Library loaded
                return true;
            }
        }
    }
    false
}