use crate::time::Instant;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// TODO
#[derive(Debug)]
pub struct Sleep(Pin<Box<t10::time::Sleep>>);

impl Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).as_mut().poll(cx)
    }
}

impl Sleep {
    /// TODO
    pub fn deadline(&self) -> Instant {
        Instant(self.0.deadline())
    }

    /// TODO
    pub fn reset(&mut self, deadline: Instant) {
        self.0.as_mut().reset(deadline.0)
    }
    /// TODO
    pub fn is_elapsed(&mut self) -> bool {
        self.0.as_mut().is_elapsed()
    }
}

/// TODO
pub fn sleep(duration: std::time::Duration) -> Sleep {
    Sleep(Box::pin(t10::time::sleep(duration)))
}

/// TODO
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep(Box::pin(t10::time::sleep_until(deadline.0)))
}
