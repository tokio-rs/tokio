#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;

#[test]
#[cfg_attr(panic = "abort", ignore)]
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
#[cfg_attr(panic = "abort", ignore)]
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
#[cfg_attr(panic = "abort", ignore)]
fn interleave_enter_same_rt() {
    let rt1 = rt();

    let _enter1 = rt1.enter();
    let enter2 = rt1.enter();
    let enter3 = rt1.enter();

    drop(enter2);
    drop(enter3);
}

#[test]
#[cfg(not(target_os = "wasi"))]
#[cfg_attr(panic = "abort", ignore)]
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

#[cfg(tokio_unstable)]
mod unstable {
    use super::*;

    #[test]
    fn runtime_id_is_same() {
        let rt = rt();

        let handle1 = rt.handle();
        let handle2 = rt.handle();

        assert_eq!(handle1.id(), handle2.id());
    }

    #[test]
    fn runtime_ids_different() {
        let rt1 = rt();
        let rt2 = rt();

        assert_ne!(rt1.handle().id(), rt2.handle().id());
    }
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}
