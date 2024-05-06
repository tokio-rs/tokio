fn main() {
    // Warning: build.rs is not published to crates.io.

    println!("cargo:rustc-cfg=check_cfg");
    println!("cargo:rustc-check-cfg=cfg(check_cfg)");
    println!("cargo:rustc-check-cfg=cfg(loom)");
}
