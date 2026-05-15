use crate::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::signal::windows::{CtrlBreak, CtrlC};

/// A wrapper around [`CtrlC`] that implements [`Stream`].
///
/// [`CtrlC`]: struct@tokio::signal::windows::CtrlC
/// [`Stream`]: trait@crate::Stream
///
/// # Example
///
/// ```no_run
/// use tokio::signal::windows::ctrl_c;
/// use tokio_stream::{StreamExt, wrappers::CtrlCStream};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> std::io::Result<()> {
/// let signals = ctrl_c()?;
/// let mut stream = CtrlCStream::new(signals);
/// while stream.next().await.is_some() {
///     println!("ctrl-c received");
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(all(windows, feature = "signal"))))]
pub struct CtrlCStream {
    inner: CtrlC,
}

impl CtrlCStream {
    /// Create a new `CtrlCStream`.
    pub fn new(interval: CtrlC) -> Self {
        Self { inner: interval }
    }

    /// Get back the inner `CtrlC`.
    pub fn into_inner(self) -> CtrlC {
        self.inner
    }
}

impl Stream for CtrlCStream {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.inner.poll_recv(cx)
    }
}

impl AsRef<CtrlC> for CtrlCStream {
    fn as_ref(&self) -> &CtrlC {
        &self.inner
    }
}

impl AsMut<CtrlC> for CtrlCStream {
    fn as_mut(&mut self) -> &mut CtrlC {
        &mut self.inner
    }
}

/// A wrapper around [`CtrlBreak`] that implements [`Stream`].
///
/// # Example
///
/// ```no_run
/// use tokio::signal::windows::ctrl_break;
/// use tokio_stream::{StreamExt, wrappers::CtrlBreakStream};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> std::io::Result<()> {
/// let signals = ctrl_break()?;
/// let mut stream = CtrlBreakStream::new(signals);
/// while stream.next().await.is_some() {
///     println!("ctrl-break received");
/// }
/// # Ok(())
/// # }
/// ```
///
/// [`CtrlBreak`]: struct@tokio::signal::windows::CtrlBreak
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(all(windows, feature = "signal"))))]
pub struct CtrlBreakStream {
    inner: CtrlBreak,
}

impl CtrlBreakStream {
    /// Create a new `CtrlBreakStream`.
    pub fn new(interval: CtrlBreak) -> Self {
        Self { inner: interval }
    }

    /// Get back the inner `CtrlBreak`.
    pub fn into_inner(self) -> CtrlBreak {
        self.inner
    }
}

impl Stream for CtrlBreakStream {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.inner.poll_recv(cx)
    }
}

impl AsRef<CtrlBreak> for CtrlBreakStream {
    fn as_ref(&self) -> &CtrlBreak {
        &self.inner
    }
}

impl AsMut<CtrlBreak> for CtrlBreakStream {
    fn as_mut(&mut self) -> &mut CtrlBreak {
        &mut self.inner
    }
}
