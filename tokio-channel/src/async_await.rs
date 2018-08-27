use {oneshot, mpsc};

use std::marker::Unpin;

impl<T> Unpin for oneshot::Sender<T> {}
impl<T> Unpin for oneshot::Receiver<T> {}

impl<T> Unpin for mpsc::Sender<T> {}
impl<T> Unpin for mpsc::UnboundedSender<T> {}
impl<T> Unpin for mpsc::Receiver<T> {}
