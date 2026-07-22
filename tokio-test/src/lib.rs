#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Tokio and Futures based testing utilities

pub mod io;
pub mod stream_mock;

mod macros;
pub mod task;

/// Runs the provided future, blocking the current thread until the
/// future completes.
///
/// For more information, see the documentation for
/// [`tokio::runtime::Runtime::block_on`][runtime-block-on].
///
/// [runtime-block-on]: https://docs.rs/tokio/1.3.0/tokio/runtime/struct.Runtime.html#method.block_on
#[cfg(not(target_os = "emscripten"))]
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use tokio::runtime;

    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(future)
}

/// Emscripten variant: polls once with a no-op waker. Ready futures (mocks,
/// pure computation) complete; ones that must yield (timers, I/O) panic —
/// use `#[tokio::test]` for those.
#[cfg(target_os = "emscripten")]
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    // SAFETY: vtable entries are valid no-ops.
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);

    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!(
            "tokio_test::block_on: future returned Pending on emscripten. \
             The main thread cannot block on JS event-loop wakeups; use #[tokio::test] \
             for futures that need timers/network I/O."
        ),
    }
}
