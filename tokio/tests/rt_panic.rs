#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use futures::future;
use parking_lot::{const_mutex, Mutex};
use std::error::Error;
use std::panic;
use std::sync::Arc;
use tokio::runtime::{Builder, Handle, Runtime};

fn test_panic<Func: FnOnce() + panic::UnwindSafe>(func: Func) -> Option<String> {
    static PANIC_MUTEX: Mutex<()> = const_mutex(());

    {
        let _guard = PANIC_MUTEX.lock();
        let panic_file: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let prev_hook = panic::take_hook();
        {
            let panic_file = panic_file.clone();
            panic::set_hook(Box::new(move |panic_info| {
                let panic_location = panic_info.location().unwrap();
                panic_file
                    .lock()
                    .clone_from(&Some(panic_location.file().to_string()));
            }));
        }

        let result = panic::catch_unwind(func);
        // Return to the previously set panic hook (maybe default) so that we get nice error
        // messages in the tests.
        panic::set_hook(prev_hook);

        if result.is_err() {
            panic_file.lock().clone()
        } else {
            None
        }
    }
}

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
        let rt = basic();
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
fn builder_worker_threads_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().worker_threads(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn builder_max_blocking_threads_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().max_blocking_threads(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

fn basic() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
