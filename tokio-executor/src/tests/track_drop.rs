use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::task::{Context, Poll};

#[derive(Debug)]
pub(crate) struct TrackDrop<T>(T, Arc<AtomicBool>);

#[derive(Debug)]
pub(crate) struct DidDrop(Arc<AtomicBool>, Arc<AtomicBool>);

pub(crate) fn track_drop<T: Future>(
    future: T,
) -> (impl Future<Output = TrackDrop<T::Output>>, DidDrop) {
    let did_drop_future = Arc::new(AtomicBool::new(false));
    let did_drop_output = Arc::new(AtomicBool::new(false));
    let did_drop = DidDrop(did_drop_future.clone(), did_drop_output.clone());

    let future = async move { TrackDrop(future.await, did_drop_output) };

    let future = TrackDrop(future, did_drop_future);

    (future, did_drop)
}

impl<T> TrackDrop<T> {
    pub(crate) fn get_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Future> Future for TrackDrop<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { Pin::map_unchecked_mut(self, |x| &mut x.0) };
        me.poll(cx)
    }
}

impl<T> Drop for TrackDrop<T> {
    fn drop(&mut self) {
        self.1.store(true, SeqCst);
    }
}

impl DidDrop {
    pub(crate) fn did_drop_future(&self) -> bool {
        self.0.load(SeqCst)
    }

    pub(crate) fn did_drop_output(&self) -> bool {
        self.1.load(SeqCst)
    }
}
