fn emit(cfg: &str) {
    println!("cargo:rustc-cfg={}", cfg);
}

fn main() {
    let target = ::std::env::var("TARGET").unwrap_or_default();

    if target.starts_with("wasm") {
        if target.contains("wasi") {
            emit("tokio_wasi");
        } else {
            emit("tokio_wasm_not_wasi");
        }
    }
}
