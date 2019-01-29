#![allow(missing_docs)]
use std::fmt;

/// A mock span.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct MockSpan {
    pub name: Option<&'static str>,
}

pub fn mock() -> MockSpan {
    MockSpan {
        ..Default::default()
    }
}

impl MockSpan {
    pub fn named(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }
}


impl fmt::Display for MockSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.name {
            Some(name) => write!(f, "a span named {:?}", name),
            None => write!(f, "any span"),
        }?;
        Ok(())
    }
}
