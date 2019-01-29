#![allow(missing_docs)]
use super::field;

use std::{borrow::Borrow, fmt};

/// A mock event.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct MockEvent {
    pub name: Option<String>,
    pub fields: Option<field::Expect>,
}

pub fn mock() -> MockEvent {
    MockEvent {
        ..Default::default()
    }
}

impl MockEvent {
    pub fn named<I>(self, name: I) -> Self
    where
        I: ToOwned<Owned = String>,
        String: Borrow<I>,
    {
        Self {
            name: Some(name.to_owned()),
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
}


impl fmt::Display for MockEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "an event")?;
        if let Some(ref name) = self.name {
            write!(f, " named {:?}", name)?;
        }
        if let Some(ref fields) = self.fields {
            write!(f, " with {}", fields)?
        }
        Ok(())
    }
}
