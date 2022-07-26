#![cfg(feature = "macros")]

#[cfg(tokio_wasm_not_wasi)]
use wasm_bindgen_test::wasm_bindgen_test as maybe_tokio_test;

#[cfg(not(tokio_wasm_not_wasi))]
use tokio::test as maybe_tokio_test;

async fn one() {}
async fn two() {}

#[maybe_tokio_test]
async fn multi_pin() {
    tokio::pin! {
        let f1 = one();
        let f2 = two();
    }

    (&mut f1).await;
    (&mut f2).await;
}
