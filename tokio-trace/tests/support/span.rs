#![allow(missing_docs)]
use super::{field, metadata};
use std::fmt;

/// A mock span.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct MockSpan {
    pub(in support) metadata: metadata::Expect,
}

#[derive(Debug, Eq, PartialEq)]
pub(in support) enum Parent {
    ContextualRoot,
    Contextual(String),
    ExplicitRoot,
    Explicit(String),
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct NewSpan {
    pub(in support) span: MockSpan,
    pub(in support) fields: field::Expect,
    pub(in support) parent: Option<Parent>,
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
            metadata: metadata::Expect {
                name: Some(name.into()),
                ..self.metadata
            },
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

    pub fn with_explicit_parent(self, parent: Option<&str>) -> NewSpan {
        let parent = match parent {
            Some(name) => Parent::Explicit(name.into()),
            None => Parent::ExplicitRoot,
        };
        NewSpan {
            parent: Some(parent),
            span: self,
            ..Default::default()
        }
    }

    pub fn with_contextual_parent(self, parent: Option<&str>) -> NewSpan {
        let parent = match parent {
            Some(name) => Parent::Contextual(name.into()),
            None => Parent::ContextualRoot,
        };
        NewSpan {
            parent: Some(parent),
            span: self,
            ..Default::default()
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.metadata.name.as_ref().map(String::as_ref)
    }

    pub fn with_field<I>(self, fields: I) -> NewSpan
    where
        I: Into<field::Expect>,
    {
        NewSpan {
            span: self,
            fields: fields.into(),
            ..Default::default()
        }
    }

    pub(in support) fn check_metadata(&self, actual: &tokio_trace::Metadata) {
        self.metadata.check(actual, format_args!("span {}", self))
    }
}

impl fmt::Display for MockSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.metadata.name.is_some() {
            write!(f, "a span{}", self.metadata)
        } else {
            write!(f, "any span{}", self.metadata)
        }
    }
}

impl Into<NewSpan> for MockSpan {
    fn into(self) -> NewSpan {
        NewSpan {
            span: self,
            ..Default::default()
        }
    }
}

impl NewSpan {
    pub fn with_explicit_parent(self, parent: Option<&str>) -> NewSpan {
        let parent = match parent {
            Some(name) => Parent::Explicit(name.into()),
            None => Parent::ExplicitRoot,
        };
        NewSpan {
            parent: Some(parent),
            ..self
        }
    }

    pub fn with_contextual_parent(self, parent: Option<&str>) -> NewSpan {
        let parent = match parent {
            Some(name) => Parent::Contextual(name.into()),
            None => Parent::ContextualRoot,
        };
        NewSpan {
            parent: Some(parent),
            ..self
        }
    }

    pub fn with_field<I>(self, fields: I) -> NewSpan
    where
        I: Into<field::Expect>,
    {
        NewSpan {
            fields: fields.into(),
            ..self
        }
    }
}

impl fmt::Display for NewSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a new span{}", self.span.metadata)?;
        if !self.fields.is_empty() {
            write!(f, " with {}", self.fields)?;
        }
        Ok(())
    }
}
