#![allow(missing_docs)]
use std::fmt;
use super::metadata;

/// A mock span.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct MockSpan {
    pub(in support) metadata: metadata::Expected,
}

pub fn mock() -> MockSpan {
    MockSpan {
        ..Default::default()
    }
}

impl MockSpan {
    pub fn named<I>(self, name: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            metadata: metadata::Expected {
                name: Some(name.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn at_level(self, level: tokio_trace::Level) -> Self {
        Self {
            metadata: metadata::Expected {
                level: Some(level),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn with_target<I>(self, target: I) -> Self
    where

        I: Into<String>,
    {
        Self {
            metadata: metadata::Expected {
                target: Some(target.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.metadata.name.as_ref().map(String::as_ref)
    }

    pub(in support) fn check_metadata(&self, actual: &tokio_trace::Metadata) {
        self.metadata.check(actual, format_args!("span {}", self))
    }
}


impl fmt::Display for MockSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.metadata.name {
            Some(ref name) => write!(f, "a span named {:?}", name),
            None => write!(f, "any span"),
        }?;
        Ok(())
    }
}
