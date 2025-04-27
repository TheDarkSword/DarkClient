use crate::platform::LIBRARY_NAME;
use dll_syringe::{process::OwnedProcess, Syringe};
use std::io;
use std::path::PathBuf;

pub fn inject(pid: u32) -> Result<(), io::Error> {
    let lib_path = PathBuf::from(format!("{}.dll", LIBRARY_NAME));

    let process = OwnedProcess::from_pid(pid).map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to get process"))?;
    let syringe = Syringe::for_process(process);
    syringe.inject(&lib_path).map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to inject library"))?;

    Ok(())
}

pub fn find_pid() -> Option<u32> {
    let output = Command::new("tasklist")
        .output()
        .expect("Failed to execute `tasklist` command");

    let output_str = String::from_utf8_lossy(&output.stdout);

    for line in output_str.lines() {
        if line.contains("minecraft") && line.contains("java") {
            if let Some(pid) = line.split_whitespace().nth(1) {
                println!("{}", pid);
            }
        }
    }
}