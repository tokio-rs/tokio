/// Condvar wrapper for tokio

use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::{Mutex, MutexGuard, Notify};

/// Behaves like std::sync::Condvar but for tokio Mutexes
pub struct Condvar {
    notifier: Notify,
    wait_count: AtomicU32,
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

impl Condvar {
    /// Create new condition variable
    pub fn new() -> Self {
        Self{ wait_count: AtomicU32::new(0), notifier: Notify::new() }
    }

    /// For for another thread to call notify or notify_all
    pub async fn wait<'a, T>(&self, lock: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        self.wait_count.fetch_add(1, Ordering::SeqCst);
        let mutex = lock.into_mutex_reference();

        self.notifier.notified().await;
        mutex.lock().await
    }

    /// Notify at most one waiter
    pub fn notify_one<T>(&self) {
        let mut count = self.wait_count.load(Ordering::SeqCst);

        while count > 0 {
            match self.wait_count.compare_exchange(count, count-1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => self.notifier.notify_one(),
                Err(val) => count = val
            }
        }
    }

    /// Notify all waiters
    pub fn notify_all(&self) {
        let count = self.wait_count.swap(0, Ordering::SeqCst);

        for _ in 0..count {
            self.notifier.notify_one();
        }
    }
}
