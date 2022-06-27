#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use futures::future;
use parking_lot::{const_mutex, Mutex};
use std::error::Error;
use std::panic;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::time::{self, interval, interval_at, timeout, Instant};

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
fn pause_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let rt = basic();

        rt.block_on(async {
            time::pause();
            time::pause();
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn resume_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let rt = basic();

        rt.block_on(async {
            time::resume();
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn interval_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = interval(Duration::from_millis(0));
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn interval_at_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = interval_at(Instant::now(), Duration::from_millis(0));
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn timeout_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        // Runtime without `enable_time` so it has no current timer set.
        let rt = Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let _ = timeout(Duration::from_millis(5), future::pending::<()>());
        });
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
