use std::{error, fmt};

/// An error that can never occur
pub enum Never {}

impl fmt::Debug for Never {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        match *self {}
    }
}

impl fmt::Display for Never {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        match *self {}
    }
}

impl error::Error for Never {
    fn description(&self) -> &str {
        match *self {}
    }
}
