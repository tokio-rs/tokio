mod maybe_dangling;
#[cfg(any(feature = "io", feature = "codec"))]
mod poll_buf;

pub(crate) use maybe_dangling::MaybeDangling;
#[cfg(any(feature = "io", feature = "codec"))]
#[cfg_attr(not(feature = "io"), allow(unreachable_pub))]
pub use poll_buf::{poll_read_buf, poll_write_buf};

cfg_rt! {
    use std::task::{Context, Poll};
    use tokio::task::coop::poll_proceed;
    use futures_core::ready;

    #[cfg_attr(not(feature = "io"), allow(unused))]
    pub(crate) fn poll_proceed_and_make_progress(cx: &mut Context<'_>) -> Poll<()> {
        ready!(poll_proceed(cx)).made_progress();
        Poll::Ready(())
    }
}

cfg_not_rt! {
    use std::task::{Context, Poll};

    #[cfg_attr(not(feature = "io"), allow(unused))]
    pub(crate) fn poll_proceed_and_make_progress(_cx: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}
