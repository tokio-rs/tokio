use std::io::{self, SeekFrom};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Seek bytes asynchronously.
///
/// This trait is analogous to the [`std::io::Seek`] trait, but integrates
/// with the asynchronous task system. In particular, the `start_seek`
/// method, unlike [`Seek::seek`], will not block the calling thread.
///
/// Utilities for working with `AsyncSeek` values are provided by
/// [`AsyncSeekExt`].
///
/// [`std::io::Seek`]: std::io::Seek
/// [`Seek::seek`]: std::io::Seek::seek()
/// [`AsyncSeekExt`]: crate::io::AsyncSeekExt
pub use t10::io:: AsyncSeek;