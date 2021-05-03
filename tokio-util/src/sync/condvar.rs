use std::sync::atomic;
use tokio::sync::{MutexGuard, Notify};

/// Behaves like std::sync::Condvar but for tokio Mutexes
#[ derive(Debug) ]
pub struct Condvar {
    notifier: Notify,
    wait_count: atomic::AtomicU32,
}

impl Condvar {
    /// Create new condition variable
    pub fn new() -> Self {
        Self{ wait_count: atomic::AtomicU32::new(0), notifier: Notify::new() }
    }

    /// For for another thread to call notify or notify_all
    pub async fn wait<'a, T>(&self, lock: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        self.wait_count.fetch_add(1, atomic::Ordering::SeqCst);
        let mutex = lock.into_mutex_reference();

        self.notifier.notified().await;
        mutex.lock().await
    }

    /// Notify at most one other thread
    pub fn notify_one(&self) {
        let count = self.wait_count.load(atomic::Ordering::SeqCst);

        if count > 0 {
            self.wait_count.store(count-1, atomic::Ordering::SeqCst);
            self.notifier.notify_one();
        }
    }

    /// Notify all waiting threads
    pub fn notify_all(&self) {
        let count = self.wait_count.swap(0, atomic::Ordering::SeqCst);

        for _ in 0..count {
            self.notifier.notify_one();
        }
    }
}
