#![cfg(not(feature = "full"))]
compile_error!("run main Tokio tests with `--features full`");
