use autocfg::AutoCfg;

fn main() {
    match AutoCfg::new() {
        Ok(ac) => {
            // Const-initialized thread locals were stabilized in 1.59
            if ac.probe_rustc_version(1, 59) {
                autocfg::emit("tokio_const_thread_local")
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
