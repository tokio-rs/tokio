use crate::io::interest::Interest;
use crate::io::ready::Ready;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::runtime::io::{Direction, ReadyEvent, Tick};
use crate::util::bit;
use crate::util::linked_list::{self, LinkedList};
use crate::util::WakeList;

use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::task::{Context, Poll, Waker};

/// Stored in the I/O driver resource slab.
#[derive(Debug)]
// # This struct should be cache padded to avoid false sharing. The cache padding rules are copied
// from crossbeam-utils/src/cache_padded.rs
//
// Starting from Intel's Sandy Bridge, spatial prefetcher is now pulling pairs of 64-byte cache
// lines at a time, so we have to align to 128 bytes rather than 64.
//
// Sources:
// - https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-optimization-manual.pdf
// - https://github.com/facebook/folly/blob/1b5288e6eea6df074758f877c849b6e73bbb9fbb/folly/lang/Align.h#L107
//
// ARM's big.LITTLE architecture has asymmetric cores and "big" cores have 128-byte cache line size.
//
// Sources:
// - https://www.mono-project.com/news/2016/09/12/arm64-icache/
//
// powerpc64 has 128-byte cache line size.
//
// Sources:
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_ppc64x.go#L9
#[cfg_attr(
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
    ),
    repr(align(128))
)]
// arm, mips, mips64, sparc, and hexagon have 32-byte cache line size.
//
// Sources:
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_arm.go#L7
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mips.go#L7
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mipsle.go#L7
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mips64x.go#L9
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/sparc/include/asm/cache.h#L17
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/hexagon/include/asm/cache.h#L12
#[cfg_attr(
    any(
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "sparc",
        target_arch = "hexagon",
    ),
    repr(align(32))
)]
// m68k has 16-byte cache line size.
//
// Sources:
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/m68k/include/asm/cache.h#L9
#[cfg_attr(target_arch = "m68k", repr(align(16)))]
// s390x has 256-byte cache line size.
//
// Sources:
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_s390x.go#L7
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/s390/include/asm/cache.h#L13
#[cfg_attr(target_arch = "s390x", repr(align(256)))]
// x86, riscv, wasm, and sparc64 have 64-byte cache line size.
//
// Sources:
// - https://github.com/golang/go/blob/dda2991c2ea0c5914714469c4defc2562a907230/src/internal/cpu/cpu_x86.go#L9
// - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_wasm.go#L7
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/sparc/include/asm/cache.h#L19
// - https://github.com/torvalds/linux/blob/3516bd729358a2a9b090c1905bd2a3fa926e24c6/arch/riscv/include/asm/cache.h#L10
//
// All others are assumed to have 64-byte cache line size.
#[cfg_attr(
    not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "sparc",
        target_arch = "hexagon",
        target_arch = "m68k",
        target_arch = "s390x",
    )),
    repr(align(64))
)]
pub(crate) struct ScheduledIo {
    pub(super) linked_list_pointers: UnsafeCell<linked_list::Pointers<Self>>,

    /// Packs the resource's readiness and I/O driver latest tick.
    readiness: AtomicUsize,

    waiters: Mutex<Waiters>,
}

#[derive(Debug, Default)]
struct Waiters {
    /// List of all current waiters.
    list: LinkedList<Waiter>,

    /// Waker used for `AsyncRead`.
    reader: Option<Waker>,

    /// Waker used for `AsyncWrite`.
    writer: Option<Waker>,
}

#[derive(Debug)]
struct Waiter {
    pointers: linked_list::Pointers<Waiter>,

    /// The waker for this task.
    waker: Option<Waker>,

    /// The interest this waiter is waiting on.
    interest: Interest,

    is_ready: bool,

    /// Should never be `Unpin`.
    _p: PhantomPinned,
}

generate_addr_of_methods! {
    impl<> Waiter {
        unsafe fn addr_of_pointers(self: NonNull<Self>) -> NonNull<linked_list::Pointers<Waiter>> {
            &self.pointers
        }
    }
}

/// Future returned by `readiness()`.
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

// The `ScheduledIo::readiness` (`AtomicUsize`) is packed full of goodness.
//
// | shutdown | write tick | read tick | readiness |
// |----------+------------+-----------+-----------|
// |   1 bit  |   7 bits   |   7 bits  |  16 bits  |
//
// The driver maintains separate generation counters ("ticks") for the read and
// write directions. A clear of read readiness is validated only against the
// read tick (and write against the write tick), so a write-only notification
// cannot invalidate a pending read-side `clear_readiness`, which previously
// caused a livelock (see <https://github.com/tokio-rs/tokio/issues/8054>).
//
// Each tick is 7 bits so that both fit alongside the 16-bit readiness and the
// shutdown bit within a `usize` on 32-bit targets (including `wasm32`). The
// pre-#6135 design used a single 8-bit tick shared across both directions and
// was sufficient; with the counters split, each direction counts only its own
// (fewer) events, so 7 bits per direction leaves equal-or-greater headroom.

const READINESS: bit::Pack = bit::Pack::least_significant(16);

const READ_TICK: bit::Pack = READINESS.then(7);

const WRITE_TICK: bit::Pack = READ_TICK.then(7);

const SHUTDOWN: bit::Pack = WRITE_TICK.then(1);

/// Packs readiness, both generation counters, and the shutdown flag into one word.
fn pack_io_state(ready: usize, read_tick: usize, write_tick: usize, shutdown: usize) -> usize {
    let mut x = READINESS.pack(ready, 0);
    x = READ_TICK.pack(read_tick, x);
    x = WRITE_TICK.pack(write_tick, x);
    SHUTDOWN.pack(shutdown, x)
}

/// Readiness bits whose clearing is validated against [`ReadyEvent::read_tick`],
/// and that advance the read tick when reported by the driver.
fn read_side_tick_mask() -> Ready {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Ready::READABLE | Ready::READ_CLOSED | Ready::ERROR | Ready::PRIORITY
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        Ready::READABLE | Ready::READ_CLOSED | Ready::ERROR
    }
}

/// Readiness bits whose clearing is validated against [`ReadyEvent::write_tick`],
/// and that advance the write tick when reported by the driver.
fn write_side_tick_mask() -> Ready {
    Ready::WRITABLE | Ready::WRITE_CLOSED
}

// ===== impl ScheduledIo =====

impl Default for ScheduledIo {
    fn default() -> ScheduledIo {
        ScheduledIo {
            linked_list_pointers: UnsafeCell::new(linked_list::Pointers::new()),
            readiness: AtomicUsize::new(0),
            waiters: Mutex::new(Waiters::default()),
        }
    }
}

impl ScheduledIo {
    pub(crate) fn token(&self) -> mio::Token {
        mio::Token(super::EXPOSE_IO.expose_provenance(self))
    }

    /// Invoked when the IO driver is shut down; forces this `ScheduledIo` into a
    /// permanently shutdown state.
    pub(super) fn shutdown(&self) {
        let mask = SHUTDOWN.pack(1, 0);
        self.readiness.fetch_or(mask, AcqRel);
        self.wake(Ready::ALL);
    }

    /// Merges readiness reported by the I/O driver into this `ScheduledIo`.
    ///
    /// The read-side and write-side generation counters advance independently
    /// based on which directions `incoming` touches, so a write-only
    /// notification does not advance the read tick (and so cannot invalidate a
    /// pending read-side [`clear_readiness`](Self::clear_readiness)). See
    /// <https://github.com/tokio-rs/tokio/issues/8054>.
    pub(super) fn merge_readiness_from_driver(&self, incoming: Ready) {
        const MAX_READ: usize = READ_TICK.max_value() + 1;
        const MAX_WRITE: usize = WRITE_TICK.max_value() + 1;

        let bump_read = !(incoming & read_side_tick_mask()).is_empty();
        let bump_write = !(incoming & write_side_tick_mask()).is_empty();

        let _ = self.readiness.fetch_update(AcqRel, Acquire, |curr| {
            // The driver only merges readiness; it never runs after shutdown.
            debug_assert!(SHUTDOWN.unpack(curr) == 0);

            let ready = Ready::from_usize(READINESS.unpack(curr)) | incoming;

            let mut read_tick = READ_TICK.unpack(curr);
            let mut write_tick = WRITE_TICK.unpack(curr);
            if bump_read {
                read_tick = read_tick.wrapping_add(1) % MAX_READ;
            }
            if bump_write {
                write_tick = write_tick.wrapping_add(1) % MAX_WRITE;
            }

            Some(pack_io_state(
                ready.as_usize(),
                read_tick,
                write_tick,
                SHUTDOWN.unpack(curr),
            ))
        });
    }

    /// Clears readiness by invoking `f` on the current readiness, but only if
    /// the generation counter(s) for the cleared direction(s) still match the
    /// snapshot in `tick_op`. The ticks themselves are left unchanged.
    pub(super) fn set_readiness(&self, tick_op: Tick, f: impl Fn(Ready) -> Ready) {
        let Tick::Clear { read, write } = tick_op;

        let _ = self.readiness.fetch_update(AcqRel, Acquire, |curr| {
            // Validate each direction's tick independently; a mismatch means the
            // driver set new readiness for that direction since the snapshot, so
            // the clear is stale and must be skipped.
            if let Some(t) = read {
                if READ_TICK.unpack(curr) as u8 != t {
                    return None;
                }
            }
            if let Some(t) = write {
                if WRITE_TICK.unpack(curr) as u8 != t {
                    return None;
                }
            }

            let ready = Ready::from_usize(READINESS.unpack(curr));
            Some(pack_io_state(
                f(ready).as_usize(),
                READ_TICK.unpack(curr),
                WRITE_TICK.unpack(curr),
                SHUTDOWN.unpack(curr),
            ))
        });
    }

    /// Notifies all pending waiters that have registered interest in `ready`.
    ///
    /// There may be many waiters to notify. Waking the pending task **must** be
    /// done from outside of the lock otherwise there is a potential for a
    /// deadlock.
    ///
    /// A stack array of wakers is created and filled with wakers to notify, the
    /// lock is released, and the wakers are notified. Because there may be more
    /// than 32 wakers to notify, if the stack array fills up, the lock is
    /// released, the array is cleared, and the iteration continues.
    pub(super) fn wake(&self, ready: Ready) {
        let mut wakers = WakeList::new();

        let mut waiters = self.waiters.lock();

        // check for AsyncRead slot
        if ready.is_readable() {
            if let Some(waker) = waiters.reader.take() {
                wakers.push(waker);
            }
        }

        // check for AsyncWrite slot
        if ready.is_writable() {
            if let Some(waker) = waiters.writer.take() {
                wakers.push(waker);
            }
        }

        'outer: loop {
            let mut iter = waiters.list.drain_filter(|w| ready.satisfies(w.interest));

            while wakers.can_push() {
                match iter.next() {
                    Some(waiter) => {
                        let waiter = unsafe { &mut *waiter.as_ptr() };

                        if let Some(waker) = waiter.waker.take() {
                            waiter.is_ready = true;
                            wakers.push(waker);
                        }
                    }
                    None => {
                        break 'outer;
                    }
                }
            }

            drop(waiters);

            wakers.wake_all();

            // Acquire the lock again.
            waiters = self.waiters.lock();
        }

        // Release the lock before notifying
        drop(waiters);

        wakers.wake_all();
    }

    pub(super) fn ready_event(&self, interest: Interest) -> ReadyEvent {
        let curr = self.readiness.load(Acquire);

        ReadyEvent {
            read_tick: READ_TICK.unpack(curr) as u8,
            write_tick: WRITE_TICK.unpack(curr) as u8,
            ready: interest.mask() & Ready::from_usize(READINESS.unpack(curr)),
            is_shutdown: SHUTDOWN.unpack(curr) != 0,
        }
    }

    /// Polls for readiness events in a given direction.
    ///
    /// These are to support `AsyncRead` and `AsyncWrite` polling methods,
    /// which cannot use the `async fn` version. This uses reserved reader
    /// and writer slots.
    pub(super) fn poll_readiness(
        &self,
        cx: &mut Context<'_>,
        direction: Direction,
    ) -> Poll<ReadyEvent> {
        let curr = self.readiness.load(Acquire);

        let ready = direction.mask() & Ready::from_usize(READINESS.unpack(curr));
        let is_shutdown = SHUTDOWN.unpack(curr) != 0;

        if ready.is_empty() && !is_shutdown {
            // Update the task info
            let mut waiters = self.waiters.lock();
            let waker = match direction {
                Direction::Read => &mut waiters.reader,
                Direction::Write => &mut waiters.writer,
            };

            // Avoid cloning the waker if one is already stored that matches the
            // current task.
            match waker {
                Some(waker) => waker.clone_from(cx.waker()),
                None => *waker = Some(cx.waker().clone()),
            }

            // Try again, in case the readiness was changed while we were
            // taking the waiters lock
            let curr = self.readiness.load(Acquire);
            let ready = direction.mask() & Ready::from_usize(READINESS.unpack(curr));
            let is_shutdown = SHUTDOWN.unpack(curr) != 0;
            if is_shutdown {
                Poll::Ready(ReadyEvent {
                    read_tick: READ_TICK.unpack(curr) as u8,
                    write_tick: WRITE_TICK.unpack(curr) as u8,
                    ready: direction.mask(),
                    is_shutdown,
                })
            } else if ready.is_empty() {
                Poll::Pending
            } else {
                Poll::Ready(ReadyEvent {
                    read_tick: READ_TICK.unpack(curr) as u8,
                    write_tick: WRITE_TICK.unpack(curr) as u8,
                    ready,
                    is_shutdown,
                })
            }
        } else {
            Poll::Ready(ReadyEvent {
                read_tick: READ_TICK.unpack(curr) as u8,
                write_tick: WRITE_TICK.unpack(curr) as u8,
                ready,
                is_shutdown,
            })
        }
    }

    pub(crate) fn clear_readiness(&self, event: ReadyEvent) {
        // This consumes the current readiness state **except** for closed
        // states. Closed states are excluded because they are final states.
        let mask_no_closed = event.ready - Ready::READ_CLOSED - Ready::WRITE_CLOSED;

        // Only validate the generation counter for the direction(s) actually
        // being cleared. A read-only clear thus is not invalidated by a
        // concurrent write-only driver event, and vice versa, which is what
        // previously caused the livelock in
        // <https://github.com/tokio-rs/tokio/issues/8054>.
        let read = if !(mask_no_closed & read_side_tick_mask()).is_empty() {
            Some(event.read_tick)
        } else {
            None
        };
        let write = if !(mask_no_closed & write_side_tick_mask()).is_empty() {
            Some(event.write_tick)
        } else {
            None
        };

        self.set_readiness(Tick::Clear { read, write }, |curr| curr - mask_no_closed);
    }

    pub(crate) fn clear_wakers(&self) {
        let mut waiters = self.waiters.lock();
        waiters.reader.take();
        waiters.writer.take();
    }
}

impl Drop for ScheduledIo {
    fn drop(&mut self) {
        self.wake(Ready::ALL);
    }
}

unsafe impl Send for ScheduledIo {}
unsafe impl Sync for ScheduledIo {}

impl ScheduledIo {
    /// An async version of `poll_readiness` which uses a linked list of wakers.
    pub(crate) async fn readiness(&self, interest: Interest) -> ReadyEvent {
        self.readiness_fut(interest).await
    }

    // This is in a separate function so that the borrow checker doesn't think
    // we are borrowing the `UnsafeCell` possibly over await boundaries.
    //
    // Go figure.
    fn readiness_fut(&self, interest: Interest) -> Readiness<'_> {
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

    unsafe fn pointers(target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        unsafe { Waiter::addr_of_pointers(target) }
    }
}

// ===== impl Readiness =====

impl Future for Readiness<'_> {
    type Output = ReadyEvent;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::sync::atomic::Ordering::SeqCst;

        let (scheduled_io, state, waiter) = {
            // Safety: `Self` is `!Unpin`
            //
            // While we could use `pin_project!` to remove
            // this unsafe block, there are already unsafe blocks here,
            // so it wouldn't significantly ease the mental burden
            // and would actually complicate the code.
            // That's why we didn't use it.
            let me = unsafe { self.get_unchecked_mut() };
            (me.scheduled_io, &mut me.state, &me.waiter)
        };

        loop {
            match *state {
                State::Init => {
                    // Optimistically check existing readiness
                    let curr = scheduled_io.readiness.load(SeqCst);
                    let is_shutdown = SHUTDOWN.unpack(curr) != 0;

                    // Safety: `waiter.interest` never changes
                    let interest = unsafe { (*waiter.get()).interest };
                    let ready = Ready::from_usize(READINESS.unpack(curr)).intersection(interest);

                    if !ready.is_empty() || is_shutdown {
                        // Currently ready!
                        *state = State::Done;
                        return Poll::Ready(ReadyEvent {
                            read_tick: READ_TICK.unpack(curr) as u8,
                            write_tick: WRITE_TICK.unpack(curr) as u8,
                            ready,
                            is_shutdown,
                        });
                    }

                    // Wasn't ready, take the lock (and check again while locked).
                    let mut waiters = scheduled_io.waiters.lock();

                    let curr = scheduled_io.readiness.load(SeqCst);
                    let mut ready = Ready::from_usize(READINESS.unpack(curr));
                    let is_shutdown = SHUTDOWN.unpack(curr) != 0;

                    if is_shutdown {
                        ready = Ready::ALL;
                    }

                    let ready = ready.intersection(interest);

                    if !ready.is_empty() || is_shutdown {
                        // Currently ready!
                        *state = State::Done;
                        return Poll::Ready(ReadyEvent {
                            read_tick: READ_TICK.unpack(curr) as u8,
                            write_tick: WRITE_TICK.unpack(curr) as u8,
                            ready,
                            is_shutdown,
                        });
                    }

                    // Not ready even after locked, insert into list...

                    // Safety: Since the `waiter` is not in the intrusive list yet,
                    // we have exclusive access to it. The Mutex ensures
                    // that this modification is visible to other threads that
                    // acquire the same Mutex.
                    let waker = unsafe { &mut (*waiter.get()).waker };
                    let old = waker.replace(cx.waker().clone());
                    debug_assert!(old.is_none(), "waker should be None at the first poll");

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

                    let waiters = scheduled_io.waiters.lock();

                    // Safety: With the lock held, we have exclusive access to
                    // the waiter. In other words, `ScheduledIo::wake()`
                    // cannot access the waiter concurrently.
                    let w = unsafe { &mut *waiter.get() };

                    if w.is_ready {
                        // Our waker has been notified.
                        *state = State::Done;
                    } else {
                        // Update the waker, if necessary.
                        w.waker.as_mut().unwrap().clone_from(cx.waker());
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
                    let curr = scheduled_io.readiness.load(Acquire);
                    let is_shutdown = SHUTDOWN.unpack(curr) != 0;

                    // The returned tick might be newer than the event
                    // which notified our waker. This is ok because the future
                    // still didn't return `Poll::Ready`.

                    // Safety: We don't need to acquire the lock here because
                    //   1. `State::Done`` means `waiter` is no longer shared,
                    //      this means no concurrent access to `waiter` can happen
                    //      at this point.
                    //   2. `waiter.interest` is never changed, this means
                    //      no side effects need to be synchronized by the lock.
                    let interest = unsafe { (*waiter.get()).interest };
                    // The readiness state could have been cleared in the meantime,
                    // but we allow the returned ready set to be empty.
                    let ready = Ready::from_usize(READINESS.unpack(curr)).intersection(interest);

                    return Poll::Ready(ReadyEvent {
                        read_tick: READ_TICK.unpack(curr) as u8,
                        write_tick: WRITE_TICK.unpack(curr) as u8,
                        ready,
                        is_shutdown,
                    });
                }
            }
        }
    }
}

impl Drop for Readiness<'_> {
    fn drop(&mut self) {
        let mut waiters = self.scheduled_io.waiters.lock();

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

#[cfg(all(test, not(loom)))]
mod tests {
    use super::ScheduledIo;
    use crate::io::interest::Interest;
    use crate::io::ready::Ready;

    // A write-only driver notification must not invalidate a pending read-side
    // `clear_readiness`. This is the livelock reported in
    // <https://github.com/tokio-rs/tokio/issues/8054>: read readiness stays set
    // while reads return `WouldBlock`, so `poll_read` spins instead of parking.
    #[test]
    fn write_event_does_not_block_read_clear() {
        let io = ScheduledIo::default();

        // Driver reports the resource is readable; snapshot it as a reader would.
        io.merge_readiness_from_driver(Ready::READABLE);
        let read_event = io.ready_event(Interest::READABLE);
        assert!(read_event.ready.is_readable());

        // The read returns `WouldBlock`; before the reader clears its readiness,
        // the driver delivers a write-only event (the spurious `POLL_SEND`
        // wakeup on Windows), advancing only the write generation counter.
        io.merge_readiness_from_driver(Ready::WRITABLE);

        // Clearing read readiness with the earlier snapshot must succeed because
        // the read tick is unchanged.
        io.clear_readiness(read_event);

        assert!(
            !io.ready_event(Interest::READABLE).ready.is_readable(),
            "read readiness should have been cleared"
        );
        // The write readiness delivered in between must be untouched.
        assert!(io.ready_event(Interest::WRITABLE).ready.is_writable());
    }

    // The symmetric case: a read-only event must not block a write clear.
    #[test]
    fn read_event_does_not_block_write_clear() {
        let io = ScheduledIo::default();

        io.merge_readiness_from_driver(Ready::WRITABLE);
        let write_event = io.ready_event(Interest::WRITABLE);
        assert!(write_event.ready.is_writable());

        io.merge_readiness_from_driver(Ready::READABLE);

        io.clear_readiness(write_event);

        assert!(
            !io.ready_event(Interest::WRITABLE).ready.is_writable(),
            "write readiness should have been cleared"
        );
        assert!(io.ready_event(Interest::READABLE).ready.is_readable());
    }

    // A *same-direction* event must still invalidate a stale clear, preserving
    // the generation-counter (ABA) protection that prevents a clear from
    // consuming readiness the driver set after the snapshot was taken.
    #[test]
    fn same_direction_event_still_blocks_stale_clear() {
        let io = ScheduledIo::default();

        io.merge_readiness_from_driver(Ready::READABLE);
        let stale = io.ready_event(Interest::READABLE);

        // Another readable event arrives before the clear runs.
        io.merge_readiness_from_driver(Ready::READABLE);

        // The clear is stale (read tick advanced), so readiness is preserved.
        io.clear_readiness(stale);
        assert!(
            io.ready_event(Interest::READABLE).ready.is_readable(),
            "stale read clear must not consume newer readiness"
        );
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::ScheduledIo;
    use crate::io::interest::Interest;
    use crate::io::ready::Ready;
    use crate::loom::sync::Arc;

    // Models the issue #8054 race directly: a "reader" snapshots read readiness
    // and clears it (as `poll_read` does after `WouldBlock`) while the I/O
    // driver delivers a concurrent write-only event. With separate read/write
    // generation counters, the write event advances only the write tick, so the
    // read clear must take effect in **every** interleaving — read readiness can
    // never be stranded (which previously spun `poll_read` into a livelock).
    #[test]
    fn write_event_never_strands_read_clear() {
        loom::model(|| {
            let io = Arc::new(ScheduledIo::default());
            // The driver has already reported the resource readable.
            io.merge_readiness_from_driver(Ready::READABLE);

            let driver = {
                let io = io.clone();
                loom::thread::spawn(move || {
                    // A write-only notification (e.g. the spurious `POLL_SEND`
                    // wakeup on Windows) lands concurrently with the clear.
                    io.merge_readiness_from_driver(Ready::WRITABLE);
                })
            };

            // Reader: observe readiness, then clear it after a `WouldBlock` read.
            let ev = io.ready_event(Interest::READABLE);
            io.clear_readiness(ev);

            driver.join().unwrap();

            assert!(
                !io.ready_event(Interest::READABLE).ready.is_readable(),
                "read readiness stranded by a concurrent write event (issue #8054)"
            );
        });
    }
}
