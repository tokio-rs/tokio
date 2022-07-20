#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))]

use futures::future;
use std::error::Error;
use tokio::{runtime::Builder, spawn, task};

mod support {
    pub mod panic;
}
use support::panic::test_panic;

#[test]
fn local_set_block_on_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let local = task::LocalSet::new();

        rt.block_on(async {
            local.block_on(&rt, future::pending::<()>());
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn spawn_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        spawn(future::pending::<()>());
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn local_key_sync_scope_panic_caller() -> Result<(), Box<dyn Error>> {
    tokio::task_local! {
        static NUMBER: u32;
    }

    let panic_location_file = test_panic(|| {
        NUMBER.sync_scope(1, || {
            NUMBER.with(|_| {
                let _ = NUMBER.sync_scope(1, || {});
            });
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn local_key_with_panic_caller() -> Result<(), Box<dyn Error>> {
    tokio::task_local! {
        static NUMBER: u32;
    }

    let panic_location_file = test_panic(|| {
        NUMBER.with(|_| {});
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn local_key_get_panic_caller() -> Result<(), Box<dyn Error>> {
    tokio::task_local! {
        static NUMBER: u32;
    }

    let panic_location_file = test_panic(|| {
        NUMBER.get();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}
