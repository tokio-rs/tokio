mod block;
mod chan;
mod list;
mod rx;
mod tx;

pub use self::tx::{Sender, SendError};
pub use self::rx::{Receiver};

pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = chan::channel(buffer);

    let tx = Sender::new(tx);
    let rx = Receiver::new(rx);

    (tx, rx)
}
