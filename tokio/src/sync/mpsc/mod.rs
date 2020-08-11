#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

//! A multi-producer, single-consumer queue for sending values across
//! asynchronous tasks.
//!
//! Similar to `std`, channel creation provides [`Receiver`] and [`Sender`]
//! handles. [`Receiver`] implements `Stream` and allows a task to read values
//! out of the channel. If there is no message to read, the current task will be
//! notified when a new value is sent. If the channel is at capacity, the send
//! is rejected and the task will be notified when additional capacity is
//! available. In other words, the channel provides backpressure.
//!
//! This module provides two variants of the channel: bounded and unbounded. The
//! bounded variant has a limit on the number of messages that the channel can
//! store, and if this limit is reached, trying to send another message will
//! wait until a message is received from the channel. An unbounded channel has
//! an infinite capacity, so the `send` method never does any kind of sleeping.
//! This makes the [`UnboundedSender`] usable from both synchronous and
//! asynchronous code.
//!
//! # Disconnection
//!
//! When all [`Sender`] handles have been dropped, it is no longer
//! possible to send values into the channel. This is considered the termination
//! event of the stream. As such, `Receiver::poll` returns `Ok(Ready(None))`.
//!
//! If the [`Receiver`] handle is dropped, then messages can no longer
//! be read out of the channel. In this case, all further attempts to send will
//! result in an error.
//!
//! # Clean Shutdown
//!
//! When the [`Receiver`] is dropped, it is possible for unprocessed messages to
//! remain in the channel. Instead, it is usually desirable to perform a "clean"
//! shutdown. To do this, the receiver first calls `close`, which will prevent
//! any further messages to be sent into the channel. Then, the receiver
//! consumes the channel to completion, at which point the receiver can be
//! dropped.
//!
//! # Communicating between sync and async code
//!
//! When you want to communicate between synchronous and asynchronous code, there
//! are two situations to consider:
//!
//! **Bounded channel**: If you need a bounded channel, you should use a bounded
//! Tokio `mpsc` channel for both directions of communication. Instead of calling
//! the async [`send`][bounded-send] or [`recv`][bounded-recv] methods, in
//! synchronous code you will need to use the [`blocking_send`][blocking-send] or
//! [`blocking_recv`][blocking-recv] methods.
//!
//! **Unbounded channel**: You should use the kind of channel that matches where
//! the receiver is. So for sending a message _from async to sync_, you should
//! use [the standard library unbounded channel][std-unbounded] or
//! [crossbeam][crossbeam-unbounded].  Similarly, for sending a message _from sync
//! to async_, you should use an unbounded Tokio `mpsc` channel.
//!
//! [`Sender`]: crate::sync::mpsc::Sender
//! [`Receiver`]: crate::sync::mpsc::Receiver
//! [bounded-send]: crate::sync::mpsc::Sender::send()
//! [bounded-recv]: crate::sync::mpsc::Receiver::recv()
//! [blocking-send]: crate::sync::mpsc::Sender::blocking_send()
//! [blocking-recv]: crate::sync::mpsc::Receiver::blocking_recv()
//! [`UnboundedSender`]: crate::sync::mpsc::UnboundedSender
//! [`Handle::block_on`]: crate::runtime::Handle::block_on()
//! [std-unbounded]: std::sync::mpsc::channel
//! [crossbeam-unbounded]: https://docs.rs/crossbeam/*/crossbeam/channel/fn.unbounded.html

pub(super) mod block;

mod bounded;
pub use self::bounded::{channel, Receiver, Sender};

mod chan;

pub(super) mod list;

mod unbounded;
pub use self::unbounded::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub mod error;

/// The number of values a block can contain.
///
/// This value must be a power of 2. It also must be smaller than the number of
/// bits in `usize`.
#[cfg(all(target_pointer_width = "64", not(loom)))]
const BLOCK_CAP: usize = 32;

#[cfg(all(not(target_pointer_width = "64"), not(loom)))]
const BLOCK_CAP: usize = 16;

#[cfg(loom)]
const BLOCK_CAP: usize = 2;
