use autocfg::AutoCfg;

const CONST_THREAD_LOCAL_PROBE: &str = r#"
{
    thread_local! {
        static MY_PROBE: usize = const { 10 };
    }

    MY_PROBE.with(|val| *val)
}
"#;

const ADDR_OF_PROBE: &str = r#"
{
    let my_var = 10;
    ::std::ptr::addr_of!(my_var)
}
"#;

const CONST_MUTEX_NEW_PROBE: &str = r#"
{
    static MY_MUTEX: ::std::sync::Mutex<i32> = ::std::sync::Mutex::new(1);
    *MY_MUTEX.lock().unwrap()
}
"#;

fn main() {
    let mut enable_const_thread_local = false;
    let mut enable_addr_of = false;
    let mut enable_const_mutex_new = false;

    match AutoCfg::new() {
        Ok(ac) => {
            // These checks prefer to call only `probe_rustc_version` if that is
            // enough to determine whether the feature is supported. This is
            // because the `probe_expression` call involves a call to rustc,
            // which the `probe_rustc_version` call avoids.

            // Const-initialized thread locals were stabilized in 1.59.
            if ac.probe_rustc_version(1, 60) {
                enable_const_thread_local = true;
            } else if ac.probe_rustc_version(1, 59) {
                // This compiler claims to be 1.59, but there are some nightly
                // compilers that claim to be 1.59 without supporting the
                // feature. Explicitly probe to check if code using them
                // compiles.
                //
                // The oldest nightly that supports the feature is 2021-12-06.
                if ac.probe_expression(CONST_THREAD_LOCAL_PROBE) {
                    enable_const_thread_local = true;
                }
            }

            // The `addr_of` and `addr_of_mut` macros were stabilized in 1.51.
            if ac.probe_rustc_version(1, 52) {
                enable_addr_of = true;
            } else if ac.probe_rustc_version(1, 51) {
                // This compiler claims to be 1.51, but there are some nightly
                // compilers that claim to be 1.51 without supporting the
                // feature. Explicitly probe to check if code using them
                // compiles.
                //
                // The oldest nightly that supports the feature is 2021-01-31.
                if ac.probe_expression(ADDR_OF_PROBE) {
                    enable_addr_of = true;
                }
            }

            // The `Mutex::new` method was made const in 1.63.
            if ac.probe_rustc_version(1, 64) {
                enable_const_mutex_new = true;
            } else if ac.probe_rustc_version(1, 63) {
                // This compiler claims to be 1.63, but there are some nightly
                // compilers that claim to be 1.63 without supporting the
                // feature. Explicitly probe to check if code using them
                // compiles.
                //
                // The oldest nightly that supports the feature is 2022-06-20.
                if ac.probe_expression(CONST_MUTEX_NEW_PROBE) {
                    enable_const_mutex_new = true;
                }
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

    if !enable_const_thread_local {
        // To disable this feature on compilers that support it, you can
        // explicitly pass this flag with the following environment variable:
        //
        // RUSTFLAGS="--cfg tokio_no_const_thread_local"
        autocfg::emit("tokio_no_const_thread_local")
    }

    if !enable_addr_of {
        // To disable this feature on compilers that support it, you can
        // explicitly pass this flag with the following environment variable:
        //
        // RUSTFLAGS="--cfg tokio_no_addr_of"
        autocfg::emit("tokio_no_addr_of")
    }

    if !enable_const_mutex_new {
        // To disable this feature on compilers that support it, you can
        // explicitly pass this flag with the following environment variable:
        //
        // RUSTFLAGS="--cfg tokio_no_const_mutex_new"
        autocfg::emit("tokio_no_const_mutex_new")
    }

    let target = ::std::env::var("TARGET").unwrap_or_default();

    // We emit cfgs instead of using `target_family = "wasm"` that requires Rust 1.54.
    // Note that these cfgs are unavailable in `Cargo.toml`.
    if target.starts_with("wasm") {
        autocfg::emit("tokio_wasm");
        if target.contains("wasi") {
            autocfg::emit("tokio_wasi");
        } else {
            autocfg::emit("tokio_wasm_not_wasi");
        }
    }
}
