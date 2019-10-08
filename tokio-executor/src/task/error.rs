use std::any::Any;
use std::fmt;

/// Task failed to execute to completion.
#[derive(Debug)]
pub struct Error {
    repr: Repr,
}

#[derive(Debug)]
enum Repr {
    Cancelled,
    Panic(Box<dyn Any + Send + 'static>),
}

impl Error {
    /// Create a new `cancelled` error
    pub fn cancelled() -> Error {
        Error { repr: Repr::Cancelled }
    }

    /// Create a new `panic` error
    pub fn panic(err: Box<dyn Any + Send + 'static>) -> Error {
        Error { repr: Repr::Panic(err) }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            Repr::Cancelled => write!(fmt, "cancelled"),
            Repr::Panic(_) => write!(fmt, "panic"),
        }
    }
}

impl std::error::Error for Error {
}
