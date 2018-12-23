use super::chan;

use futures::{Poll, Stream};

use std::sync::Arc;

pub struct Receiver<T> {
    /// The channel receiver
    chan: chan::Rx<T>,
}

impl<T> Receiver<T> {
    pub(crate) fn new(chan: chan::Rx<T>) -> Receiver<T> {
        Receiver { chan }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<T>, Self::Error> {
        unimplemented!();
    }
}
