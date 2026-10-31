#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(target_os = "emscripten", feature = "rt", feature = "macros")
))]
#![cfg(not(target_os = "wasi"))] // Wasi doesn't support panic recovery
#![cfg(panic = "unwind")]

use futures::future;
use std::error::Error;
#[cfg(feature = "rt-multi-thread")]
use tokio::runtime::Builder;
use tokio::runtime::{Handle, Runtime};

mod support {
    pub mod panic;
}
use support::panic::test_panic;

#[test]
fn current_handle_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Handle::current();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn into_panic_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(move || {
        let rt = current_thread();
        rt.block_on(async {
            let handle = tokio::spawn(future::pending::<()>());

            handle.abort();

            let err = handle.await.unwrap_err();
            assert!(!&err.is_panic());

            let _ = err.into_panic();
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
#[cfg(feature = "rt-multi-thread")]
fn builder_worker_threads_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().worker_threads(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
#[cfg(feature = "rt-multi-thread")]
fn builder_max_blocking_threads_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().max_blocking_threads(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
#[cfg(feature = "rt-multi-thread")]
fn builder_global_queue_interval_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().global_queue_interval(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
#[cfg(feature = "rt-multi-thread")]
fn builder_event_interval_interval_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().event_interval(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
#[cfg(feature = "rt-multi-thread")]
fn builder_name_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().name(" ").build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn waker_panic_in_driver_turn_leaves_runtime_usable() -> Result<(), Box<dyn Error>> {
    use std::future::Future;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;
    use std::task::{Context, Wake, Waker};
    use std::time::Duration;

    struct PanicWaker;

    impl Wake for PanicWaker {
        fn wake(self: Arc<Self>) {
            panic!("waker panicked");
        }
    }

    let rt = current_thread();

    // Register a timer whose waker panics when the timer fires.
    let mut sleep = {
        let _enter = rt.enter();
        Box::pin(tokio::time::sleep(Duration::from_millis(1)))
    };
    let waker = Waker::from(Arc::new(PanicWaker));
    assert!(sleep
        .as_mut()
        .poll(&mut Context::from_waker(&waker))
        .is_pending());

    // The timer fires inside the driver turn, so the panic unwinds out of `block_on`.
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
    }));
    assert!(panicked.is_err());

    // The driver must be back in the core: the runtime still parks, and dropping
    // it must not trip the "never placed the Core back" check.
    rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });

    drop(sleep);
    drop(rt);

    Ok(())
}

fn current_thread() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
