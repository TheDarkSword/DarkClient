// build.rs
// This build script is only relevant on Windows with MSVC toolchain.
// It finds the `jvm.lib` import library that is required to link JNI functions.
// On Linux, this is unnecessary because the linker can directly use libjvm.so.

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    use std::{env, fs};

    println!("cargo:rerun-if-env-changed=JAVA_HOME");
    println!("cargo:rerun-if-env-changed=JVM_LIB_DIR");

    // --- 1) Explicit override: JVM_LIB_DIR environment variable ---
    if let Ok(dir) = env::var("JVM_LIB_DIR") {
        let p = PathBuf::from(&dir);
        if p.exists() {
            link_jvm(&p);
            println!("cargo:warning=Using JVM_LIB_DIR={}", dir);
            return;
        }
    }

    // --- 2) JAVA_HOME environment variable ---
    if let Ok(java_home) = env::var("JAVA_HOME") {
        let p = PathBuf::from(java_home);
        if let Some(found) = search_jvm_in(&p) {
            link_jvm(&found);
            println!(
                "cargo:warning=Found jvm.lib via JAVA_HOME at {}",
                found.display()
            );
            return;
        }
    }

    // --- 3) Common installation directories ---
    let common = [r"C:\Program Files\Java", r"C:\Program Files (x86)\Java"];

    for root in &common {
        let rootp = PathBuf::from(root);
        if let Ok(entries) = fs::read_dir(&rootp) {
            for e in entries.flatten() {
                if let Some(found) = search_jvm_in(&e.path()) {
                    link_jvm(&found);
                    println!("cargo:warning=Found jvm.lib in {}", found.display());
                    return;
                }
            }
        }
    }

    // --- 4) Windows Registry lookup ---
    if let Some(found) = find_jvm_from_registry() {
        link_jvm(&found);
        println!(
            "cargo:warning=Found jvm.lib via Windows Registry at {}",
            found.display()
        );
        return;
    }

    // --- 5) Nothing found: fail build with instructions ---
    panic!(
        "build.rs: could not find jvm.lib. 
    - Set JAVA_HOME to your JDK root (e.g. C:\\Program Files\\Java\\jdk-21)
    - Or set JVM_LIB_DIR directly to the folder containing jvm.lib"
    );
}

#[cfg(not(windows))]
fn main() {
    // On non-Windows systems this build script does nothing.
}

#[cfg(windows)]
// Adds the directory to the linker search path and tells Cargo to link against jvm.lib
fn link_jvm(dir: &std::path::PathBuf) {
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=jvm");
}

#[cfg(windows)]
// Tries common subpaths of a JDK installation to find jvm.lib
fn search_jvm_in(root: &std::path::PathBuf) -> Option<std::path::PathBuf> {
    let tries = [
        root.join("lib").join("jvm.lib"),
        root.join("lib").join("amd64").join("jvm.lib"),
        root.join("lib").join("x86_64").join("jvm.lib"),
        root.join("lib").join("server").join("jvm.lib"),
        root.join("lib").join("client").join("jvm.lib"),
    ];
    for candidate in &tries {
        if candidate.exists() {
            return candidate.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

// Tries to query Windows Registry for the JDK installation path
#[cfg(windows)]
fn find_jvm_from_registry() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    use std::process::Command;

    let output = Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\JavaSoft\JDK", "/s"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("JavaHome") {
            let parts: Vec<_> = line.split_whitespace().collect();
            if let Some(path) = parts.last() {
                let p = PathBuf::from(path);
                if let Some(found) = search_jvm_in(&p) {
                    return Some(found);
                }
            }
        }
    }
    None
}
