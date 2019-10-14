use loom::sync::Notify;

use std::sync::{Arc, Mutex};

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        notify: Notify::new(),
        value: Mutex::new(None),
    });

    let tx = Sender {
        inner: inner.clone(),
    };
    let rx = Receiver { inner };

    (tx, rx)
}

pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
}

struct Inner<T> {
    notify: Notify,
    value: Mutex<Option<T>>,
}

impl<T> Sender<T> {
    pub fn send(self, value: T) {
        *self.inner.value.lock().unwrap() = Some(value);
        self.inner.notify.notify();
    }
}

impl<T> Receiver<T> {
    pub fn recv(self) -> T {
        loop {
            if let Some(v) = self.inner.value.lock().unwrap().take() {
                return v;
            }

            self.inner.notify.wait();
        }
    }
}
