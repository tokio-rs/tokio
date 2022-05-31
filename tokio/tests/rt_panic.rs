#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use lazy_static::lazy_static;
use std::error::Error;
use std::panic;
use std::sync::{Arc, Mutex, Once};
use tokio::runtime::{Builder, Handle, Runtime};

fn test_panic<Func: FnOnce() + panic::UnwindSafe>(func: Func) -> Option<String> {
    lazy_static! {
        static ref SET_UP_PANIC_HOOK: Once = Once::new();
        static ref PANIC_MUTEX: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        static ref PANIC_FILE: Mutex<String> = Mutex::new(String::new());
    }

    SET_UP_PANIC_HOOK.call_once(|| {
        panic::set_hook(Box::new(|panic_info| {
            let panic_location = panic_info.location().unwrap();
            PANIC_FILE
                .lock()
                .unwrap()
                .clone_from(&panic_location.file().to_string());
        }));
    });

    {
        let _guard = PANIC_MUTEX.lock();

        let result = panic::catch_unwind(func);

        if result.is_err() {
            Some(PANIC_FILE.lock().ok()?.clone())
        } else {
            None
        }
    }
}

#[test]
fn test_current_handle_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Handle::current();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn test_into_panic_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(move || {
        let rt = basic();
        rt.block_on(async {
            let handle = tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            });

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
fn test_builder_worker_threads_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = Builder::new_multi_thread().worker_threads(0).build();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn test_builder_max_blocking_threads_panic_caller() -> Result<(), Box<dyn Error>> {
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
