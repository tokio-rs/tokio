use std::env;

const ENV_TRACE_ENABLED: &'static str = "TOKIO_TRACE_ENABLED";
const TRACE_FEATURE: &'static str = "trace";

fn main() {
    if let Ok(_) = env::var(ENV_TRACE_ENABLED) {
        println!("cargo:rustc-cfg=feature=\"{}\"", TRACE_FEATURE);
    }

    println!("cargo:rerun-if-env-changed={}", ENV_TRACE_ENABLED);
}
