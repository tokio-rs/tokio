//! Thread local runtime context
use crate::runtime::Handle;
use std::fmt::Write;

use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<Handle>> = RefCell::new(None)
}

pub(crate) fn current() -> Option<Handle> {
    CONTEXT.with(|ctx| ctx.borrow().clone())
}

cfg_io_driver! {
    pub(crate) fn io_handle() -> crate::runtime::driver::IoHandle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.io_handle.clone(),
            None => Default::default(),
        })
    }
}

cfg_signal_internal! {
    #[cfg(unix)]
    pub(crate) fn signal_handle() -> crate::runtime::driver::SignalHandle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.signal_handle.clone(),
            None => Default::default(),
        })
    }
}

cfg_time! {
    pub(crate) fn time_handle() -> crate::runtime::driver::TimeHandle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.time_handle.clone(),
            None => Default::default(),
        })
    }

    cfg_test_util! {
        pub(crate) fn clock() -> Option<crate::runtime::driver::Clock> {
            CONTEXT.with(|ctx| match *ctx.borrow() {
                Some(ref ctx) => Some(ctx.clock.clone()),
                None => None,
            })
        }
    }
}

cfg_rt! {
    pub(crate) fn spawn_handle() -> Option<crate::runtime::Spawner> {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => Some(ctx.spawner.clone()),
            None => None,
        })
    }
}

/// Set this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn enter(new: Handle) -> EnterGuard {
    CONTEXT.with(|ctx| {
        let old = ctx.borrow_mut().replace(new);
        EnterGuard(old)
    })
}

#[derive(Debug)]
pub(crate) struct EnterGuard(Option<Handle>);

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = self.0.take();
        });
    }
}

/// Returns an error string explaining that the Tokio context hasn't been instantiated.
pub(crate) fn missing_error(features: &[&str]) -> String {
    // TODO: Include Tokio version
    let sfx = if features.len() > 0 {
        let mut sfx = String::from(" with ");
        for (i, feat) in features.iter().enumerate() {
            if i == 0 {
                if features.len() > 1 {
                    write!(&mut sfx, "either ").expect("failed to write to string");
                }
            } else if i == features.len() - 1 {
                write!(&mut sfx, " or ").expect("failed to write to string");
            } else {
                write!(&mut sfx, ", ").expect("failed to write to string");
            }
            write!(&mut sfx, "{}", feat).expect("failed to write to string");
        }
        write!(&mut sfx, " enabled").expect("failed to write to string");
        sfx
    } else {
        String::new()
    };
    format!(
        "there is no reactor running, must be called from the context of Tokio 1.x runtime{}",
        sfx
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_error_no_features() {
        assert_eq!(
            &missing_error(&[]),
            "there is no reactor running, must be called from the context of Tokio 1.x runtime"
        );
    }

    #[test]
    fn test_missing_error_one_feature() {
        assert_eq!(&missing_error(&["rt"]), 
                   "there is no reactor running, must be called from the context of Tokio 1.x runtime with rt enabled");
    }

    #[test]
    fn test_missing_error_two_features() {
        assert_eq!(&missing_error(&["rt", "signal"]), 
                "there is no reactor running, must be called from the context of Tokio 1.x runtime with either rt or signal enabled");
    }

    #[test]
    fn test_missing_error_three_features() {
        assert_eq!(&missing_error(&["rt", "signal", "sync"]), 
                   "there is no reactor running, must be called from the context of Tokio 1.x runtime with either rt, signal or sync enabled"
                );
    }
}
