#![warn(rust_2018_idioms)]
#![cfg(feature = "rt")]

use tokio::runtime::Builder;

#[test]
fn has_time_driver_is_false_when_disabled() {
    let runtime = Builder::new_current_thread().build().unwrap();

    assert!(!runtime.handle().has_time_driver());

    #[cfg(feature = "rt-multi-thread")]
    {
        let runtime = Builder::new_multi_thread().build().unwrap();

        assert!(!runtime.handle().has_time_driver());
    }
}

#[test]
fn has_time_driver_is_preserved_by_handle_clones() {
    let runtime = Builder::new_current_thread().build().unwrap();
    let handle = runtime.handle().clone();

    assert!(!handle.has_time_driver());
}

#[cfg(feature = "time")]
#[test]
fn has_time_driver_is_true_when_enabled() {
    let runtime = Builder::new_current_thread().enable_time().build().unwrap();
    let handle = runtime.handle().clone();

    assert!(runtime.handle().has_time_driver());
    assert!(handle.has_time_driver());

    runtime.block_on(async {
        assert!(tokio::runtime::Handle::current().has_time_driver());
    });

    #[cfg(all(tokio_unstable, feature = "rt-multi-thread"))]
    {
        let runtime = Builder::new_multi_thread()
            .enable_alt_timer()
            .build()
            .unwrap();
        let handle = runtime.handle().clone();

        assert!(runtime.handle().has_time_driver());
        assert!(handle.has_time_driver());

        runtime.block_on(async {
            assert!(tokio::runtime::Handle::current().has_time_driver());
        });
    }

    #[cfg(feature = "rt-multi-thread")]
    {
        let runtime = Builder::new_multi_thread().enable_time().build().unwrap();
        let handle = runtime.handle().clone();

        assert!(runtime.handle().has_time_driver());
        assert!(handle.has_time_driver());

        runtime.block_on(async {
            assert!(tokio::runtime::Handle::current().has_time_driver());
        });
    }
}

#[cfg(feature = "time")]
#[test]
fn has_time_driver_reports_the_current_runtime() {
    assert!(tokio::runtime::Handle::try_current().is_err());

    let disabled = Builder::new_current_thread().build().unwrap();
    let enabled = Builder::new_current_thread().enable_time().build().unwrap();

    let disabled_guard = disabled.enter();
    assert!(!tokio::runtime::Handle::current().has_time_driver());

    {
        let enabled_guard = enabled.enter();
        assert!(tokio::runtime::Handle::current().has_time_driver());
        drop(enabled_guard);
    }

    assert!(!tokio::runtime::Handle::current().has_time_driver());
    drop(disabled_guard);
    assert!(tokio::runtime::Handle::try_current().is_err());
}

#[cfg(feature = "time")]
#[test]
fn has_time_driver_can_guard_timeout_creation() {
    let runtime = Builder::new_current_thread().build().unwrap();
    let handle = runtime.handle().clone();

    let result = runtime.block_on(async {
        if handle.has_time_driver() {
            Some(tokio::time::timeout(std::time::Duration::from_secs(1), async { 42 }).await)
        } else {
            None
        }
    });

    assert!(result.is_none());

    let runtime = Builder::new_current_thread().enable_time().build().unwrap();
    let handle = runtime.handle().clone();

    let result = runtime.block_on(async {
        if handle.has_time_driver() {
            Some(tokio::time::timeout(std::time::Duration::from_secs(1), async { 42 }).await)
        } else {
            None
        }
    });

    assert_eq!(result.unwrap().unwrap(), 42);
}

#[cfg(feature = "time")]
#[test]
fn has_time_driver_reports_configuration_after_shutdown() {
    let runtime = Builder::new_current_thread().enable_time().build().unwrap();
    let handle = runtime.handle().clone();

    drop(runtime);

    assert!(handle.has_time_driver());

    #[cfg(feature = "rt-multi-thread")]
    {
        let runtime = Builder::new_multi_thread().enable_time().build().unwrap();
        let handle = runtime.handle().clone();

        drop(runtime);

        assert!(handle.has_time_driver());
    }

    #[cfg(all(tokio_unstable, feature = "rt-multi-thread"))]
    {
        let runtime = Builder::new_multi_thread()
            .enable_alt_timer()
            .build()
            .unwrap();
        let handle = runtime.handle().clone();

        drop(runtime);

        assert!(handle.has_time_driver());
    }
}
