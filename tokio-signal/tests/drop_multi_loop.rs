#![cfg(unix)]
#![warn(rust_2018_idioms)]

use libc;

pub mod support;
use crate::support::*;

#[test]
fn dropping_loops_does_not_cause_starvation() {
    let (mut rt, signal) = {
        let kind = SignalKind::sigusr1();

        let mut first_rt = CurrentThreadRuntime::new().expect("failed to init first runtime");
        let mut first_signal = Signal::new(kind).expect("failed to register first signal");

        let mut second_rt = CurrentThreadRuntime::new().expect("failed to init second runtime");
        let mut second_signal = Signal::new(kind).expect("failed to register second signal");

        send_signal(libc::SIGUSR1);

        let _ = run_with_timeout(&mut first_rt, first_signal.next())
            .expect("failed to await first signal");

        let _ = run_with_timeout(&mut second_rt, second_signal.next())
            .expect("failed to await second signal");

        drop(first_rt);
        drop(first_signal);

        (second_rt, second_signal)
    };

    send_signal(libc::SIGUSR1);

    let _ = run_with_timeout(&mut rt, signal.into_future());
}
