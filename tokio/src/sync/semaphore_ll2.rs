use crate::loom::{
    alloc,
    future::AtomicWaker,
    sync::{atomic::AtomicUsize, Mutex},
};
use crate::util::linked_list::{self, LinkedList};

use std::{
    pin::Pin,
    ptr::NonNull,
    sync::atomic::Ordering,
    task::{
        Context, Poll,
        Poll::{Pending, Ready},
    },
};

pub(crate) struct Semaphore {
    waiters: Mutex<LinkedList<alloc::Track<Waiter>>>,
    permits: AtomicUsize,
    add_lock: AtomicUsize,
}

/// Error returned by `Permit::poll_acquire`.
#[derive(Debug)]
pub(crate) struct AcquireError(());

pin_project_lite::pin_project! {
    pub(crate) struct Permit {
        #[pin]
        node: WaitNode,
        state: PermitState,
    }
}

type WaitNode = linked_list::Entry<alloc::Track<Waiter>>;

/// Permit state
#[derive(Debug, Copy, Clone)]
enum PermitState {
    /// Currently waiting for permits to be made available and assigned to the
    /// waiter.
    Waiting(u16),

    /// The number of acquired permits
    Acquired(u16),
}

struct Waiter {
    waker: AtomicWaker,
    needed: u16,
}

const CLOSED: usize = std::usize::MAX;

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits
    ///
    /// # Panics
    ///
    /// Panics if `permits` is zero.
    pub(crate) fn new(permits: usize) -> Self {
        assert!(permits > 0, "number of permits must be greater than 0");
        Self {
            permits: AtomicUsize::new(permits),
            waiters: Mutex::new(LinkedList::new()),
            add_lock: AtomicUsize::new(0),
        }
    }

    /// Returns the current number of available permits
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }

    /// Closes the semaphore. This prevents the semaphore from issuing new
    /// permits and notifies all pending waiters.
    pub(crate) fn close(&self) {
        // Acquire the `add_lock`, setting the "closed" flag on the lock.
        let prev = self.add_lock.fetch_or(1, Ordering::AcqRel);

        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }

        self.add_permits_locked(0, true);
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn add_permits(&self, n: usize) {
        if n == 0 {
            return;
        }

        // TODO: Handle overflow. A panic is not sufficient, the process must
        // abort.
        let prev = self.add_lock.fetch_add(n << 1, Ordering::AcqRel);
        if prev != 0 {
            // Another thread has the lock and will be responsible for notifying
            // pending waiters.
            return;
        }
        self.add_permits_locked(n, false);
    }

    fn add_permits_locked(&self, mut rem: usize, mut closed: bool) {
        while rem > 0 || closed {
            if closed {
                SemState::fetch_set_closed(&self.state, AcqRel);
            }

            // Release the permits and notify
            let mut waiters = self.waiters.lock().unwrap();
            while rem > 0 {
                let pop = {
                    let mut last = match waiters.last_mut() {
                        Some(mut last) => last,
                        None => {
                            self.permits.fetch_add(rem, Ordering::Release);
                            break;
                        }
                    };
                    if rem >= last.needed {
                        rem -= last.needed;
                        last.needed = 0;
                        true
                    } else {
                        last.neede -= rem;
                        break;
                    }
                };
                if pop {
                    waiters.pop_back().unwrap().get_mut().waker.wake();
                }
            }

            let n = rem << 1;

            let actual = if closed {
                let actual = self.add_lock.fetch_sub(n | 1, AcqRel);
                closed = false;
                actual
            } else {
                let actual = self.add_lock.fetch_sub(n, AcqRel);
                closed = actual & 1 == 1;
                actual
            };

            rem = (actual >> 1) - rem;
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        needed: u16,
        node: Pin<&mut WaitNode>,
    ) -> Poll<Result<(), AcquireError>> {
        let mut curr = self.permits.load(Ordering::Acquire);
        let remaining = loop {
            if curr == CLOSED {
                return Ready(Err(AcquireError(())));
            }
            let next = curr.saturating_sub(needed as usize);
            match self.permits.compare_exchange_weak(
                curr,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) if curr >= needed as usize => return Ready(Ok(())),
                Ok(_) => break needed - curr as u16,
                Err(actual) => curr = actual,
            }
        };
        // if we've not returned already, then we need to push the waiter to the
        // wait queue.
        assert!(node.is_unlinked());
        {
            let mut waiter = unsafe { &mut *node.get() }.get_mut();
            waiter.needed = remaining;
            waiter.waker.register_by_ref(cx.waker());
        }

        let mut queue = self.waiters.lock().unwrap();
        unsafe {
            queue.push_front(node);
        }

        Pending
    }
}

impl Permit {
    /// Creates a new `Permit`.
    ///
    /// The permit begins in the "unacquired" state.
    pub(crate) fn new() -> Permit {
        use PermitState::Acquired;

        Permit {
            node: linked_list::Entry::new(alloc::Track::new(Waiter {
                waker: AtomicWaker::new(),
                needed: 0,
            })),
            state: Acquired(0),
        }
    }

    /// Returns `true` if the permit has been acquired
    pub(crate) fn is_acquired(&self) -> bool {
        match self.state {
            PermitState::Acquired(num) if num > 0 => true,
            _ => false,
        }
    }

    /// Tries to acquire the permit. If no permits are available, the current task
    /// is notified once a new permit becomes available.
    pub(crate) fn poll_acquire(
        &mut self,
        cx: &mut Context<'_>,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Poll<Result<(), AcquireError>> {
        use std::cmp::Ordering::*;
        use PermitState::*;

        match self.state {
            Waiting(requested) => {
                // There must be a waiter
                let waiter = self.waiter.as_ref().unwrap();

                match requested.cmp(&num_permits) {
                    Less => {
                        let delta = num_permits - requested;

                        // Request additional permits. If the waiter has been
                        // dequeued, it must be re-queued.
                        if !waiter.try_inc_permits_to_acquire(delta as usize) {
                            let waiter = NonNull::from(&**waiter);

                            // Ignore the result. The check for
                            // `permits_to_acquire()` will converge the state as
                            // needed
                            let _ = semaphore.poll_acquire2(delta, || Some(waiter))?;
                        }

                        self.state = Waiting(num_permits);
                    }
                    Greater => {
                        let delta = requested - num_permits;
                        let to_release = waiter.try_dec_permits_to_acquire(delta as usize);

                        semaphore.add_permits(to_release);
                        self.state = Waiting(num_permits);
                    }
                    Equal => {}
                }

                if waiter.permits_to_acquire()? == 0 {
                    self.state = Acquired(requested);
                    return Ready(Ok(()));
                }

                waiter.waker.register_by_ref(cx.waker());

                if waiter.permits_to_acquire()? == 0 {
                    self.state = Acquired(requested);
                    return Ready(Ok(()));
                }

                Pending
            }
            Acquired(acquired) => {
                if acquired >= num_permits {
                    Ready(Ok(()))
                } else {
                    match semaphore.poll_acquire(cx, num_permits - acquired, self)? {
                        Ready(()) => {
                            self.state = Acquired(num_permits);
                            Ready(Ok(()))
                        }
                        Pending => {
                            self.state = Waiting(num_permits);
                            Pending
                        }
                    }
                }
            }
        }
    }

    /// Tries to acquire the permit.
    pub(crate) fn try_acquire(
        &mut self,
        num_permits: u16,
        semaphore: &Semaphore,
    ) -> Result<(), TryAcquireError> {
        unimplemented!()
    }

    /// Releases a permit back to the semaphore
    pub(crate) fn release(&mut self, n: u16, semaphore: &Semaphore) {
        let n = self.forget(n);
        semaphore.add_permits(n as usize);
    }

    /// Forgets the permit **without** releasing it back to the semaphore.
    ///
    /// After calling `forget`, `poll_acquire` is able to acquire new permit
    /// from the sempahore.
    ///
    /// Repeatedly calling `forget` without associated calls to `add_permit`
    /// will result in the semaphore losing all permits.
    ///
    /// Will forget **at most** the number of acquired permits. This number is
    /// returned.
    pub(crate) fn forget(&mut self, n: u16) -> u16 {
        unimplemented!()
    }
}

impl Default for Permit {
    fn default() -> Self {
        Self::new()
    }
}
