/// Assert option is some
macro_rules! assert_some {
    ($e:expr) => {{
        match $e {
            Some(v) => v,
            _ => panic!("expected some, was none"),
        }
    }};
}

/// Assert option is none
macro_rules! assert_none {
    ($e:expr) => {{
        match $e {
            Some(v) => panic!("expected none, was {:?}", v),
            _ => {}
        }
    }};
}
