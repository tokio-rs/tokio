fn main() {
    // Warning: build.rs is not published to crates.io.

    println!("cargo:rustc-cfg=check_cfg");
    println!("cargo:rustc-check-cfg=cfg(check_cfg)");
    println!("cargo:rustc-check-cfg=cfg(fs)");
    println!("cargo:rustc-check-cfg=cfg(fuzzing)");
    println!("cargo:rustc-check-cfg=cfg(loom)");
    println!("cargo:rustc-check-cfg=cfg(mio_unsupported_force_poll_poll)");
    println!("cargo:rustc-check-cfg=cfg(tokio_internal_mt_counters)");
    println!("cargo:rustc-check-cfg=cfg(tokio_no_parking_lot)");
    println!("cargo:rustc-check-cfg=cfg(tokio_no_tuning_tests)");
    println!("cargo:rustc-check-cfg=cfg(tokio_taskdump)");
    println!("cargo:rustc-check-cfg=cfg(tokio_unstable)");
}
