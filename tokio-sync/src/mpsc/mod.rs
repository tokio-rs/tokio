mod block;
mod bounded;
mod chan;
mod list;
// mod unbounded;

pub use self::bounded::{channel, Receiver, Sender, SendError};

/// The number of values a block can contain.
///
/// This value must be a power of 2. It also must be smaller than the number of
/// bits in `usize`.
#[cfg(target_pointer_width = "64")]
const BLOCK_CAP: usize = 32;

#[cfg(not(target_pointer_width = "64"))]
const BLOCK_CAP: usize = 16;
