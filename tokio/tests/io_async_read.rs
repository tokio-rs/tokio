#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use tokio::io::AsyncRead;

#[test]
fn assert_obj_safe() {
    fn _assert<T>() {}
    _assert::<Box<dyn AsyncRead>>();
}
