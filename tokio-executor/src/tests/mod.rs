//! Testing utilities

#[cfg(not(loom))]
pub(crate) mod backoff;

#[cfg(loom)]
pub(crate) mod loom_oneshot;

#[cfg(loom)]
pub(crate) mod loom_schedule;

#[cfg(not(loom))]
pub(crate) mod mock_park;

pub(crate) mod mock_schedule;

#[cfg(not(loom))]
pub(crate) mod track_drop;

/// Panic if expression results in `None`.
#[macro_export]
macro_rules! assert_some {
    ($e:expr) => {{
        match $e {
            Some(v) => v,
            _ => panic!("expected some, was none"),
        }
    }};
}

/// Panic if expression results in `Some`.
#[macro_export]
macro_rules! assert_none {
    ($e:expr) => {{
        match $e {
            Some(v) => panic!("expected none, was {:?}", v),
            _ => {}
        }
    }};
}
