/// Reads bytes asynchronously.
///
/// This trait is analogous to [`std::io::BufRead`], but integrates with
/// the asynchronous task system. In particular, the [`poll_fill_buf`] method,
/// unlike [`BufRead::fill_buf`], will automatically queue the current task for wakeup
/// and return if data is not yet available, rather than blocking the calling
/// thread.
///
/// Utilities for working with `AsyncBufRead` values are provided by
/// [`AsyncBufReadExt`].
///
/// [`std::io::BufRead`]: std::io::BufRead
/// [`poll_fill_buf`]: AsyncBufRead::poll_fill_buf
/// [`BufRead::fill_buf`]: std::io::BufRead::fill_buf
/// [`AsyncBufReadExt`]: crate::io::AsyncBufReadExt
pub use t10::io::AsyncBufRead;
