use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(tokio_nightly)");
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("failed to run rustc --version");
    let version = String::from_utf8_lossy(&output.stdout);
    if version.contains("nightly") || version.contains("dev") {
        println!("cargo:rustc-cfg=tokio_nightly");
    }
}
