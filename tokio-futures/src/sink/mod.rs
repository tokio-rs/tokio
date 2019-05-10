//! Use sinks with `async` / `await`.

mod send;

pub use self::send::Send;

use futures::Sink;

/// An extension trait which adds utility methods to `Sink` types.
pub trait SinkExt: Sink {
    /// Send an item into the sink.
    ///
    /// Note that, **because of the flushing requirement, it is usually better
    /// to batch together items to send via `send_all`, rather than flushing
    /// between each item.**
    fn send_async(&mut self, item: Self::SinkItem) -> Send<'_, Self>
    where
        Self: Sized + Unpin,
    {
        Send::new(self, item)
    }
}

impl<T: Sink> SinkExt for T {}
