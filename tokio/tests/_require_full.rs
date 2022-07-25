#![cfg(not(any(feature = "full", target_arch = "wasm")))]
compile_error!("run main Tokio tests with `--features full`");
