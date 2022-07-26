#![cfg(not(any(feature = "full", target_family = "wasm")))]
compile_error!("run main Tokio tests with `--features full`");
