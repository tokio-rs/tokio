#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;

#[test]
fn basic_enter() {
    let rt1 = rt();
    let rt2 = rt();

    let enter1 = rt1.enter();
    let enter2 = rt2.enter();

    drop(enter2);
    drop(enter1);
}

#[test]
#[should_panic]
fn interleave_enter_different_rt() {
    let rt1 = rt();
    let rt2 = rt();

    let enter1 = rt1.enter();
    let enter2 = rt2.enter();

    drop(enter1);
    drop(enter2);
}

#[test]
#[should_panic]
fn interleave_enter_same_rt() {
    let rt1 = rt();

    let _enter1 = rt1.enter();
    let enter2 = rt1.enter();
    let enter3 = rt1.enter();

    drop(enter2);
    drop(enter3);
}

#[test]
#[cfg(not(tokio_wasi))]
fn interleave_then_enter() {
    let _ = std::panic::catch_unwind(|| {
        let rt1 = rt();
        let rt2 = rt();

        let enter1 = rt1.enter();
        let enter2 = rt2.enter();

        drop(enter1);
        drop(enter2);
    });

    // Can still enter
    let rt3 = rt();
    let _enter = rt3.enter();
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}
