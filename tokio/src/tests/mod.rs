#[macro_export]
/// Assert option is some
macro_rules! assert_some {
    ($e:expr) => {{
        match $e {
            Some(v) => v,
            _ => panic!("expected some, was none"),
        }
    }};
}

#[macro_export]
/// Assert option is none
macro_rules! assert_none {
    ($e:expr) => {{
        match $e {
            Some(v) => panic!("expected none, was {:?}", v),
            _ => {}
        }
    }};
}

#[cfg(not(loom))]
pub(crate) mod backoff;

#[cfg(loom)]
pub(crate) mod loom_schedule;

pub(crate) mod mock_schedule;

#[cfg(not(loom))]
pub(crate) mod track_drop;
