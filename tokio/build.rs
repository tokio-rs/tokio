fn main() {
    println!("cargo::rustc-check-cfg=cfg(nightly)");
    // Check if we're using nightly
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("Failed to get rustc version");

    let version = String::from_utf8_lossy(&output.stdout);
    if version.contains("nightly") {
        println!("cargo:rustc-cfg=nightly");
    }
}
