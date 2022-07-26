#![cfg(not(any(feature = "full", tokio_wasm)))]
compile_error!("run main Tokio tests with `--features full`");
