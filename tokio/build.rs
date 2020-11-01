use autocfg::AutoCfg;

fn main() {
    match AutoCfg::new() {
        Ok(ac) => {
            // The #[track_caller] attribute was stabilized in rustc 1.46.0.
            if ac.probe_rustc_version(1, 46) {
                autocfg::emit("tokio_track_caller")
            }
        }

        Err(e) => {
            // If we couldn't detect the compiler version and features, just
            // print a warning. This isn't a fatal error: we can still build
            // Tokio, we just can't enable cfgs automatically.
            println!(
                "cargo:warning=tokio: failed to detect compiler features: {}",
                e
            );
        }
    }
}
