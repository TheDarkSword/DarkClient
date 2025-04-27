use crate::platform::{LIBRARY_NAME, SOCKET_ADDRESS};
use log::{error, info};
use proc_maps::get_process_maps;
use ptrace_inject::{Injector, Process};
use std::io::{Error, ErrorKind, Write};
use std::net::TcpStream;
use std::path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn inject(pid: u32) -> Result<(), Error> {
    let lib_path = PathBuf::from(format!("{}.so", LIBRARY_NAME));

    if find_library(pid, LIBRARY_NAME) {
        info!("Library already loaded");
        match TcpStream::connect(SOCKET_ADDRESS) {
            Ok(mut stream) => {
                info!("Connected to {}. Sending reload", SOCKET_ADDRESS);

                if let Err(e) = stream.write("reload".as_bytes()) {
                    error!("Failed to send reload: {:?}", e);
                }
            },
            Err(e) => {
                error!("Failed to connect to server: {:?}", e);
            }
        }
        return Ok(());
    }
    
    match Command::new("jcmd")
        .arg(pid.to_string())
        .arg("JVMTI.agent_load")
        .arg(format!("{:?}", path::absolute(&lib_path)?))
        .output() {
        Ok(output) if output.status.success() => {
            info!("Agent loaded via jcmd: {:?}", lib_path);
            return Ok(());
        }
        Ok(output) => {
            error!("jcmd failed (stderr): {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => {
            error!("Failed to execute jcmd: {:?}", e);
        }
    }

    let process = Process::get(pid).map_err(|e| {
        error!("Failed to get process: {:?}", e);
        Error::new(ErrorKind::Other, "Failed to get process")
    })?;
    let mut attach = Injector::attach(process).map_err(|e| {
        error!("Failed to attach to process: {:?}", e);
        Error::new(ErrorKind::Other, "Failed to attach to process: ")
    })?;

    attach.inject(&lib_path).map_err(|e| {
        error!("Failed to inject library: {:?}", e);
        Error::new(ErrorKind::Other, "Failed to inject library")
    })?;

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
            if path.ends_with(&format!("{}.so", lib_name)) {
                // Library loaded
                return true;
            }
        }
    }
    false
}