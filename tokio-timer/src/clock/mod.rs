//! TODO: Dox

mod clock;
mod now;

pub use self::clock::{Clock, now, with_default};
pub use self::now::Now;
