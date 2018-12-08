//! A scalable reader-writer lock.
//!
//! This implementation makes read operations faster and more scalable due to less contention,
//! while making write operations slower. It also incurs much higher memory overhead than
//! traditional reader-writer locks.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use std::thread::{self, ThreadId};

use crossbeam_utils::CachePadded;
use num_cpus;
use parking_lot;

/// A scalable read-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any point in time. The
/// write portion of this lock typically allows modification of the underlying data (exclusive
/// access) and the read portion of this lock typically allows for read-only access (shared
/// access).
///
/// This reader-writer lock differs from typical implementations in that it internally creates a
/// list of reader-writer locks called 'shards'. Shards are aligned and padded to the cache line
/// size.
///
/// Read operations lock only one shard specific to the current thread, while write operations lock
/// every shard in succession. This strategy makes concurrent read operations faster due to less
/// contention, but write operations are slower due to increased amount of locking.
pub struct RwLock<T> {
    /// A list of locks protecting the internal data.
    shards: Vec<CachePadded<parking_lot::RwLock<()>>>,

    /// The internal data.
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Creates a new `RwLock` initialized with `value`.
    pub fn new(value: T) -> RwLock<T> {
        // The number of shards is a power of two so that the modulo operation in `read` becomes a
        // simple bitwise "and".
        let num_shards = num_cpus::get().next_power_of_two();

        RwLock {
            shards: (0..num_shards)
                .map(|_| CachePadded::new(parking_lot::RwLock::new(())))
                .collect(),
            value: UnsafeCell::new(value),
        }
    }

    /// Locks this `RwLock` with shared read access, blocking the current thread until it can be
    /// acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which hold the lock.
    /// There may be other readers currently inside the lock when this method returns. This method
    /// does not provide any guarantees with respect to the ordering of whether contentious readers
    /// or writers will acquire the lock first.
    ///
    /// Returns an RAII guard which will release this thread's shared access once it is dropped.
    pub fn read(&self) -> RwLockReadGuard<T> {
        // Take the current thread index and map it to a shard index. Thread indices will tend to
        // distribute shards among threads equally, thus reducing contention due to read-locking.
        let shard_index = thread_index() & (self.shards.len() - 1);

        RwLockReadGuard {
            parent: self,
            _guard: self.shards[shard_index].read(),
            _marker: PhantomData,
        }
    }

    /// Locks this rwlock with exclusive write access, blocking the current thread until it can be
    /// acquired.
    ///
    /// This function will not return while other writers or other readers currently have access to
    /// the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock when dropped.
    pub fn write(&self) -> RwLockWriteGuard<T> {
        // Write-lock each shard in succession.
        for shard in &self.shards {
            // The write guard is forgotten, but the lock will be manually unlocked in `drop`.
            mem::forget(shard.write());
        }

        RwLockWriteGuard {
            parent: self,
            _marker: PhantomData,
        }
    }
}

/// A guard used to release the shared read access of a `RwLock` when dropped.
pub struct RwLockReadGuard<'a, T: 'a> {
    parent: &'a RwLock<T>,
    _guard: parking_lot::RwLockReadGuard<'a, ()>,
    _marker: PhantomData<parking_lot::RwLockReadGuard<'a, T>>,
}

unsafe impl<'a, T: Sync> Sync for RwLockReadGuard<'a, T> {}

impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.parent.value.get() }
    }
}

/// A guard used to release the exclusive write access of a `RwLock` when dropped.
pub struct RwLockWriteGuard<'a, T: 'a> {
    parent: &'a RwLock<T>,
    _marker: PhantomData<parking_lot::RwLockWriteGuard<'a, T>>,
}

unsafe impl<'a, T: Sync> Sync for RwLockWriteGuard<'a, T> {}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        // Unlock the shards in reverse order of locking.
        for shard in self.parent.shards.iter().rev() {
            unsafe {
                shard.force_unlock_write();
            }
        }
    }
}

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.parent.value.get() }
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.parent.value.get() }
    }
}

/// Returns a `usize` that identifies the current thread.
///
/// Each thread is associated with an 'index'. Indices usually tend to be consecutive numbers
/// between 0 and the number of running threads, but there are no guarantees. During TLS teardown
/// the associated index might change.
#[inline]
pub fn thread_index() -> usize {
    REGISTRATION.try_with(|reg| reg.index).unwrap_or(0)
}

/// The global registry keeping track of registered threads and indices.
struct ThreadIndices {
    /// Mapping from `ThreadId` to thread index.
    mapping: HashMap<ThreadId, usize>,

    /// A list of free indices.
    free_list: Vec<usize>,

    /// The next index to allocate if the free list is empty.
    next_index: usize,
}

lazy_static! {
    static ref THREAD_INDICES: Mutex<ThreadIndices> = Mutex::new(ThreadIndices {
        mapping: HashMap::new(),
        free_list: Vec::new(),
        next_index: 0,
    });
}

/// A registration of a thread with an index.
///
/// When dropped, unregisters the thread and frees the reserved index.
struct Registration {
    index: usize,
    thread_id: ThreadId,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut indices = THREAD_INDICES.lock().unwrap();
        indices.mapping.remove(&self.thread_id);
        indices.free_list.push(self.index);
    }
}

thread_local! {
    static REGISTRATION: Registration = {
        let thread_id = thread::current().id();
        let mut indices = THREAD_INDICES.lock().unwrap();

        let index = match indices.free_list.pop() {
            Some(i) => i,
            None => {
                let i = indices.next_index;
                indices.next_index += 1;
                i
            }
        };
        indices.mapping.insert(thread_id, index);

        Registration {
            index,
            thread_id,
        }
    };
}
