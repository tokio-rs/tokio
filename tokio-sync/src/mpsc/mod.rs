mod block;
mod chan;
mod list;
mod rx;
mod tx;
// mod unbounded;

pub use self::tx::{Sender, SendError};
pub use self::rx::{Receiver};

pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    let semaphore = (::Semaphore::new(buffer), buffer);
    let (tx, rx) = chan::channel(semaphore);

    let tx = Sender::new(tx);
    let rx = Receiver::new(rx);

    (tx, rx)
}

/// The number of values a block can contain.
///
/// This value must be a power of 2. It also must be smaller than the number of
/// bits in `usize`.
#[cfg(target_pointer_width = "64")]
const BLOCK_CAP: usize = 32;

#[cfg(not(target_pointer_width = "64"))]
const BLOCK_CAP: usize = 16;
