#![allow(missing_docs)]
use super::{field, metadata};

use std::fmt;

/// A mock event.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct MockEvent {
    pub fields: Option<field::Expect>,
    metadata: metadata::Expect,
}

pub fn mock() -> MockEvent {
    MockEvent {
        ..Default::default()
    }
}

impl MockEvent {
    pub fn named<I>(self, name: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            metadata: metadata::Expect {
                name: Some(name.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn with_fields<I>(self, fields: I) -> Self
    where
        I: Into<field::Expect>,
    {
        Self {
            fields: Some(fields.into()),
            ..self
        }
    }

    pub fn at_level(self, level: tokio_trace::Level) -> Self {
        Self {
            metadata: metadata::Expect {
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
            metadata: metadata::Expect {
                target: Some(target.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub(in support) fn check(self, event: &tokio_trace::Event) {
        let meta = event.metadata();
        let name = meta.name();
        self.metadata.check(meta, format_args!("event {}", name));
        assert!(meta.is_event(), "expected an event but got {:?}", event);
        if let Some(mut expected_fields) = self.fields {
            let mut checker = expected_fields.checker(format!("{}", name));
            event.record(&mut checker);
            checker.finish();
        }
    }
}

impl fmt::Display for MockEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "an event")?;
        if let Some(ref name) = self.metadata.name {
            write!(f, " named {:?}", name)?;
        }
        if let Some(ref fields) = self.fields {
            write!(f, " with {}", fields)?
        }
        Ok(())
    }
}
