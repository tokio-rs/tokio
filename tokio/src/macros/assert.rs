/// Asserts option is some
macro_rules! assert_some {
    ($e:expr) => {{
        match $e {
            Some(v) => v,
            _ => panic!("expected some, was none"),
        }
    }};
}

/// Asserts option is none
macro_rules! assert_none {
    ($e:expr) => {{
        if let Some(v) = $e {
            panic!("expected none, was {:?}", v);
        }
    }};
}
