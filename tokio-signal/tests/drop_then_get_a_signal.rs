#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn drop_then_get_a_signal() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let signal = run_core_with_timeout(&mut lp, Signal::with_handle(
        libc::SIGUSR1,
        &handle.new_tokio_handle(),
    )).expect("failed to create first signal");
    drop(signal);

    send_signal(libc::SIGUSR1);
    let signal = lp.run(Signal::with_handle(libc::SIGUSR1, &handle.new_tokio_handle()))
        .expect("failed to create signal")
        .into_future()
        .map(|_| ())
        .map_err(|(e, _)| panic!("{}", e));

    run_core_with_timeout(&mut lp, signal)
        .expect("failed to get signal");
}
