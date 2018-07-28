extern crate libc;
extern crate futures;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_signal;

use self::libc::{c_int, getpid, kill};
use std::time::{Duration, Instant};
use self::tokio::timer::Deadline;
use self::tokio_core::reactor::Timeout;

pub use self::futures::{Future, Stream};
pub use self::tokio_core::reactor::Core;
pub use self::tokio::runtime::current_thread::Runtime as CurrentThreadRuntime;
pub use self::tokio_signal::unix::Signal;

pub fn run_core_with_timeout<F>(lp: &mut Core, future: F) -> Result<F::Item, F::Error>
    where F: Future
{
    let timeout = Timeout::new(Duration::from_secs(1), &lp.handle())
        .expect("failed to register timeout")
        .map(|()| panic!("timeout exceeded"))
        .map_err(|e| panic!("timeout error: {}", e));

    lp.run(future.select(timeout))
        .map(|(r, _)| r)
        .map_err(|(e, _)| e)
}

pub fn run_with_timeout<F>(rt: &mut CurrentThreadRuntime, future: F) -> Result<F::Item, F::Error>
    where F: Future
{
    let deadline = Deadline::new(future, Instant::now() + Duration::from_secs(1))
        .map_err(|e| if e.is_timer() {
            panic!("failed to register timer");
        } else if e.is_elapsed() {
            panic!("timed out")
        } else {
            e.into_inner().expect("missing inner error")
        });

    rt.block_on(deadline)
}

#[cfg(unix)]
pub fn send_signal(signal: c_int) {
    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}
