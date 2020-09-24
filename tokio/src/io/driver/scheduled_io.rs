use super::{platform, Direction, ReadyEvent, Tick};
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::util::bit;
use crate::util::slab::Entry;

use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::task::{Context, Poll, Waker};

cfg_io_readiness! {
    use crate::util::linked_list::{self, LinkedList};

    use std::cell::UnsafeCell;
    use std::future::Future;
    use std::marker::PhantomPinned;
    use std::pin::Pin;
    use std::ptr::NonNull;
}

/// Stored in the I/O driver resource slab.
#[derive(Debug)]
pub(crate) struct ScheduledIo {
    /// Packs the resource's readiness with the resource's generation.
    readiness: AtomicUsize,

    waiters: Mutex<Waiters>,
}

cfg_io_readiness! {
    type WaitList = LinkedList<Waiter, <Waiter as linked_list::Link>::Target>;
}

#[derive(Debug, Default)]
struct Waiters {
    #[cfg(any(feature = "udp", feature = "uds"))]
    /// List of all current waiters
    list: WaitList,

    /// Waker used for AsyncRead
    reader: Option<Waker>,

    /// Waker used for AsyncWrite
    writer: Option<Waker>,
}

cfg_io_readiness! {
    #[derive(Debug)]
    struct Waiter {
        pointers: linked_list::Pointers<Waiter>,

        /// The waker for this task
        waker: Option<Waker>,

        /// The interest this waiter is waiting on
        interest: mio::Ready,

        is_ready: bool,

        /// Should never be `!Unpin`
        _p: PhantomPinned,
    }

    /// Future returned by `readiness()`
    struct Readiness<'a> {
        scheduled_io: &'a ScheduledIo,

        state: State,

        /// Entry in the waiter `LinkedList`.
        waiter: UnsafeCell<Waiter>,
    }

    enum State {
        Init,
        Waiting,
        Done,
    }
}

// The `ScheduledIo::readiness` (`AtomicUsize`) is packed full of goodness.
//
// | reserved | generation |  driver tick | readinesss |
// |----------+------------+--------------+------------|
// |   1 bit  |   7 bits   +    8 bits    +   16 bits  |

const READINESS: bit::Pack = bit::Pack::least_significant(16);

const TICK: bit::Pack = READINESS.then(8);

const GENERATION: bit::Pack = TICK.then(7);

#[test]
fn test_generations_assert_same() {
    assert_eq!(super::GENERATION, GENERATION);
}

// ===== impl ScheduledIo =====

impl Entry for ScheduledIo {
    fn reset(&self) {
        let state = self.readiness.load(Acquire);

        let generation = GENERATION.unpack(state);
        let next = GENERATION.pack_lossy(generation + 1, 0);

        self.readiness.store(next, Release);
    }
}

impl Default for ScheduledIo {
    fn default() -> ScheduledIo {
        ScheduledIo {
            readiness: AtomicUsize::new(0),
            waiters: Mutex::new(Default::default()),
        }
    }
}

impl ScheduledIo {
    pub(crate) fn generation(&self) -> usize {
        GENERATION.unpack(self.readiness.load(Acquire))
    }

    /// Sets the readiness on this `ScheduledIo` by invoking the given closure on
    /// the current value, returning the previous readiness value.
    ///
    /// # Arguments
    /// - `token`: the token for this `ScheduledIo`.
    /// - `tick`: whether setting the tick or trying to clear readiness for a
    ///    specific tick.
    /// - `f`: a closure returning a new readiness value given the previous
    ///   readiness.
    ///
    /// # Returns
    ///
    /// If the given token's generation no longer matches the `ScheduledIo`'s
    /// generation, then the corresponding IO resource has been removed and
    /// replaced with a new resource. In that case, this method returns `Err`.
    /// Otherwise, this returns the previous readiness.
    pub(super) fn set_readiness(
        &self,
        token: Option<usize>,
        tick: Tick,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let mut current = self.readiness.load(Acquire);

        loop {
            let current_generation = GENERATION.unpack(current);

            if let Some(token) = token {
                // Check that the generation for this access is still the
                // current one.
                if GENERATION.unpack(token) != current_generation {
                    return Err(());
                }
            }

            // Mask out the tick/generation bits so that the modifying
            // function doesn't see them.
            let current_readiness = current & mio::Ready::all().as_usize();
            let mut new = f(current_readiness);

            debug_assert!(
                new <= READINESS.max_value(),
                "new readiness value would overwrite tick/generation bits!"
            );

            match tick {
                Tick::Set(t) => {
                    new = TICK.pack(t as usize, new);
                }
                Tick::Clear(t) => {
                    if TICK.unpack(current) as u8 != t {
                        // Trying to clear readiness with an old event!
                        return Err(());
                    }
                    new = TICK.pack(t as usize, new);
                }
            }

            new = GENERATION.pack(current_generation, new);

            match self
                .readiness
                .compare_exchange(current, new, AcqRel, Acquire)
            {
                Ok(_) => return Ok(current),
                // we lost the race, retry!
                Err(actual) => current = actual,
            }
        }
    }

    pub(super) fn wake(&self, ready: mio::Ready) {
        let mut waiters = self.waiters.lock().unwrap();

        // check for AsyncRead slot
        if !(ready & (!mio::Ready::writable())).is_empty() {
            if let Some(waker) = waiters.reader.take() {
                waker.wake();
            }
        }

        // check for AsyncWrite slot
        if ready.is_writable() || platform::is_hup(ready) || platform::is_error(ready) {
            if let Some(waker) = waiters.writer.take() {
                waker.wake();
            }
        }

        #[cfg(any(feature = "udp", feature = "uds"))]
        {
            // check list of waiters
            for waiter in waiters
                .list
                .drain_filter(|w| !(w.interest & ready).is_empty())
            {
                let waiter = unsafe { &mut *waiter.as_ptr() };
                if let Some(waker) = waiter.waker.take() {
                    waiter.is_ready = true;
                    waker.wake();
                }
            }
        }
    }

    /// Poll version of checking readiness for a certain direction.
    ///
    /// These are to support `AsyncRead` and `AsyncWrite` polling methods,
    /// which cannot use the `async fn` version. This uses reserved reader
    /// and writer slots.
    pub(in crate::io) fn poll_readiness(
        &self,
        cx: &mut Context<'_>,
        direction: Direction,
    ) -> Poll<ReadyEvent> {
        let curr = self.readiness.load(Acquire);

        let ready = direction.mask() & mio::Ready::from_usize(READINESS.unpack(curr));

        if ready.is_empty() {
            // Update the task info
            let mut waiters = self.waiters.lock().unwrap();
            let slot = match direction {
                Direction::Read => &mut waiters.reader,
                Direction::Write => &mut waiters.writer,
            };
            *slot = Some(cx.waker().clone());

            // Try again, in case the readiness was changed while we were
            // taking the waiters lock
            let curr = self.readiness.load(Acquire);
            let ready = direction.mask() & mio::Ready::from_usize(READINESS.unpack(curr));
            if ready.is_empty() {
                Poll::Pending
            } else {
                Poll::Ready(ReadyEvent {
                    tick: TICK.unpack(curr) as u8,
                    readiness: ready,
                })
            }
        } else {
            Poll::Ready(ReadyEvent {
                tick: TICK.unpack(curr) as u8,
                readiness: ready,
            })
        }
    }

    pub(crate) fn clear_readiness(&self, event: ReadyEvent) {
        // This consumes the current readiness state **except** for HUP and
        // error. HUP and error are excluded because a) they are final states
        // and never transition out and b) both the read AND the write
        // directions need to be able to obvserve these states.
        //
        // # Platform-specific behavior
        //
        // HUP and error readiness are platform-specific. On epoll platforms,
        // HUP has specific conditions that must be met by both peers of a
        // connection in order to be triggered.
        //
        // On epoll platforms, `EPOLLERR` is signaled through
        // `UnixReady::error()` and is important to be observable by both read
        // AND write. A specific case that `EPOLLERR` occurs is when the read
        // end of a pipe is closed. When this occurs, a peer blocked by
        // writing to the pipe should be notified.
        let mask_no_hup = (event.readiness - platform::hup() - platform::error()).as_usize();

        // result isn't important
        let _ = self.set_readiness(None, Tick::Clear(event.tick), |curr| curr & (!mask_no_hup));
    }
}

impl Drop for ScheduledIo {
    fn drop(&mut self) {
        self.wake(mio::Ready::all());
    }
}

unsafe impl Send for ScheduledIo {}
unsafe impl Sync for ScheduledIo {}

cfg_io_readiness! {
    impl ScheduledIo {
        /// An async version of `poll_readiness` which uses a linked list of wakers
        pub(crate) async fn readiness(&self, interest: mio::Ready) -> ReadyEvent {
            self.readiness_fut(interest).await
        }

        // This is in a separate function so that the borrow checker doesn't think
        // we are borrowing the `UnsafeCell` possibly over await boundaries.
        //
        // Go figure.
        fn readiness_fut(&self, interest: mio::Ready) -> Readiness<'_> {
            Readiness {
                scheduled_io: self,
                state: State::Init,
                waiter: UnsafeCell::new(Waiter {
                    pointers: linked_list::Pointers::new(),
                    waker: None,
                    is_ready: false,
                    interest,
                    _p: PhantomPinned,
                }),
            }
        }
    }

    unsafe impl linked_list::Link for Waiter {
        type Handle = NonNull<Waiter>;
        type Target = Waiter;

        fn as_raw(handle: &NonNull<Waiter>) -> NonNull<Waiter> {
            *handle
        }

        unsafe fn from_raw(ptr: NonNull<Waiter>) -> NonNull<Waiter> {
            ptr
        }

        unsafe fn pointers(mut target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
            NonNull::from(&mut target.as_mut().pointers)
        }
    }

    // ===== impl Readiness =====

    impl Future for Readiness<'_> {
        type Output = ReadyEvent;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            use std::sync::atomic::Ordering::SeqCst;

            let (scheduled_io, state, waiter) = unsafe {
                let me = self.get_unchecked_mut();
                (&me.scheduled_io, &mut me.state, &me.waiter)
            };

            loop {
                match *state {
                    State::Init => {
                        // Optimistically check existing readiness
                        let curr = scheduled_io.readiness.load(SeqCst);
                        let readiness = mio::Ready::from_usize(READINESS.unpack(curr));

                        // Safety: `waiter.interest` never changes
                        let interest = unsafe { (*waiter.get()).interest };

                        if readiness.contains(interest) {
                            // Currently ready!
                            let tick = TICK.unpack(curr) as u8;
                            *state = State::Done;
                            return Poll::Ready(ReadyEvent { readiness, tick });
                        }

                        // Wasn't ready, take the lock (and check again while locked).
                        let mut waiters = scheduled_io.waiters.lock().unwrap();

                        let curr = scheduled_io.readiness.load(SeqCst);
                        let readiness = mio::Ready::from_usize(READINESS.unpack(curr));

                        if readiness.contains(interest) {
                            // Currently ready!
                            let tick = TICK.unpack(curr) as u8;
                            *state = State::Done;
                            return Poll::Ready(ReadyEvent { readiness, tick });
                        }

                        // Not ready even after locked, insert into list...

                        // Safety: called while locked
                        unsafe {
                            (*waiter.get()).waker = Some(cx.waker().clone());
                        }

                        // Insert the waiter into the linked list
                        //
                        // safety: pointers from `UnsafeCell` are never null.
                        waiters
                            .list
                            .push_front(unsafe { NonNull::new_unchecked(waiter.get()) });
                        *state = State::Waiting;
                    }
                    State::Waiting => {
                        // Currently in the "Waiting" state, implying the caller has
                        // a waiter stored in the waiter list (guarded by
                        // `notify.waiters`). In order to access the waker fields,
                        // we must hold the lock.

                        let waiters = scheduled_io.waiters.lock().unwrap();

                        // Safety: called while locked
                        let w = unsafe { &mut *waiter.get() };

                        if w.is_ready {
                            // Our waker has been notified.
                            *state = State::Done;
                        } else {
                            // Update the waker, if necessary.
                            if !w.waker.as_ref().unwrap().will_wake(cx.waker()) {
                                w.waker = Some(cx.waker().clone());
                            }

                            return Poll::Pending;
                        }

                        // Explicit drop of the lock to indicate the scope that the
                        // lock is held. Because holding the lock is required to
                        // ensure safe access to fields not held within the lock, it
                        // is helpful to visualize the scope of the critical
                        // section.
                        drop(waiters);
                    }
                    State::Done => {
                        let tick = TICK.unpack(scheduled_io.readiness.load(Acquire)) as u8;

                        // Safety: State::Done means it is no longer shared
                        let w = unsafe { &mut *waiter.get() };

                        return Poll::Ready(ReadyEvent {
                            tick,
                            readiness: w.interest,
                        });
                    }
                }
            }
        }
    }

    impl Drop for Readiness<'_> {
        fn drop(&mut self) {
            let mut waiters = self.scheduled_io.waiters.lock().unwrap();

            // Safety: `waiter` is only ever stored in `waiters`
            unsafe {
                waiters
                    .list
                    .remove(NonNull::new_unchecked(self.waiter.get()))
            };
        }
    }

    unsafe impl Send for Readiness<'_> {}
    unsafe impl Sync for Readiness<'_> {}
}
