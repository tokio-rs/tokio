#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]

pub mod support;
use support::*;

#[test]
fn dropping_loops_does_not_cause_starvation() {
    let kind = SignalKind::user_defined1();

    let mut first_rt = CurrentThreadRuntime::new().expect("failed to init first runtime");
    let mut first_signal =
        first_rt.block_on(async { signal(kind).expect("failed to register first signal") });

    let mut second_rt = CurrentThreadRuntime::new().expect("failed to init second runtime");
    let mut second_signal =
        second_rt.block_on(async { signal(kind).expect("failed to register second signal") });

    send_signal(libc::SIGUSR1);

    let _ =
        run_with_timeout(&mut first_rt, first_signal.next()).expect("failed to await first signal");

    drop(first_rt);
    drop(first_signal);

    send_signal(libc::SIGUSR1);

    let _ = run_with_timeout(&mut second_rt, second_signal.next());
}
