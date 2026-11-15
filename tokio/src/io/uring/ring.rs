use crate::io::uring::utils::ArcFd;
use crate::io::uring::{read, write};
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult};
use crate::runtime::io::{self as driver, PollOp};
use crate::runtime::Handle;
use crate::util::as_ref::OwnedBuf;

use io_uring::squeue::{Entry, Flags};
use io_uring::{opcode, types};
use std::fmt;
use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

/// The completion of one operation: its result, and the buffer of a read or
/// a write.
type Completion = (io::Result<usize>, Option<Vec<u8>>);

/// Operations to submit to the io_uring instance of a Tokio runtime.
///
/// Add operations with [`read`], [`write`], [`sync_all`], [`sync_data`] and
/// [`nop`], or one per item of an iterator with [`batch`], then submit all of
/// them at once with [`submit`]. They are
/// submitted to the runtime that polls the [`Submit`] future.
///
/// Operations don't depend on each other: they may run in parallel and
/// complete in any order. [`then`] and [`always`] link the last operation to
/// the next one, so the next one starts only after the last one completed.
///
/// An operation keeps its buffer and file descriptor until the kernel is done
/// with them, even if the future is dropped before the operation completes.
///
/// [`read`]: Ring::read
/// [`write`]: Ring::write
/// [`sync_all`]: Ring::sync_all
/// [`sync_data`]: Ring::sync_data
/// [`nop`]: Ring::nop
/// [`batch`]: Ring::batch
/// [`submit`]: Ring::submit
/// [`then`]: Ring::then
/// [`always`]: Ring::always
///
/// # Examples
///
/// Read four chunks of a file at once:
///
/// ```no_run
/// use std::sync::Arc;
/// use tokio::io::uring::Ring;
///
/// # async fn dox() -> std::io::Result<()> {
/// const CHUNK: usize = 64 * 1024;
///
/// let file = Arc::new(std::fs::File::open("data.bin")?);
/// let mut ring = Ring::new();
/// for i in 0..4 {
///     ring.read(file.clone(), Vec::with_capacity(CHUNK), i * CHUNK as u64);
/// }
///
/// for (read, buf) in ring.submit().await {
///     read?;
///     println!("read {} bytes", buf.unwrap().len());
/// }
/// # Ok(())
/// # }
/// ```
///
/// Write to a file, and flush it to disk only if the write succeeded:
///
/// ```no_run
/// use std::sync::Arc;
/// use tokio::io::uring::Ring;
///
/// # async fn dox() -> std::io::Result<()> {
/// let file = Arc::new(std::fs::File::create("log.txt")?);
///
/// let done = Ring::new()
///     .write(file.clone(), b"hello\n".to_vec(), 0)
///     .then()
///     .sync_all(file)
///     .submit()
///     .await;
///
/// for (res, _) in done {
///     res?;
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct Ring {
    /// The operations to submit next, in order.
    ops: Vec<Pending>,
}

/// An operation added to a [`Ring`], with its entry.
///
/// The entry points into heap memory owned by `op`, so it stays valid when
/// `op` is moved, until `op` is dropped.
struct Pending {
    op: RingOp,
    entry: Entry,
    /// The link from this operation to the next one, if any.
    link: Flags,
}

impl Ring {
    /// Creates a ring with no operations.
    ///
    /// This doesn't need a runtime. The operations are submitted to the
    /// runtime that polls the [`Submit`] future.
    pub fn new() -> Ring {
        Ring::default()
    }

    /// Adds a read from `fd` at `offset` into the spare capacity of `buf`.
    ///
    /// The read reads at most `buf.capacity() - buf.len()` bytes. It completes
    /// with the number of bytes read, and gives back `buf` with those bytes
    /// appended. An `offset` of `u64::MAX` reads from, and advances, the
    /// file's current position.
    pub fn read(&mut self, fd: ArcFd, buf: Vec<u8>, offset: u64) -> &mut Ring {
        let len = buf.capacity() - buf.len();
        let mut read = read::Read::new(fd, buf);
        let entry = read.sqe(len, offset);
        self.push(RingOp::Read(read), entry)
    }

    /// Adds a write of `buf` to `fd` at `offset`.
    ///
    /// The write completes with the number of bytes written, which may be less
    /// than `buf.len()`, and gives back `buf`. An `offset` of `u64::MAX` writes
    /// at, and advances, the file's current position.
    pub fn write(&mut self, fd: ArcFd, buf: Vec<u8>, offset: u64) -> &mut Ring {
        let write = write::Write::new(fd, OwnedBuf::Vec(buf));
        let entry = write.sqe(0, offset);
        self.push(RingOp::Write(write), entry)
    }

    /// Adds a flush of the data and metadata of `fd` to disk, like
    /// [`File::sync_all`].
    ///
    /// [`File::sync_all`]: std::fs::File::sync_all
    pub fn sync_all(&mut self, fd: ArcFd) -> &mut Ring {
        self.fsync(fd, types::FsyncFlags::empty())
    }

    /// Adds a flush of the data of `fd` to disk, and of the metadata needed to
    /// read it back, like [`File::sync_data`].
    ///
    /// [`File::sync_data`]: std::fs::File::sync_data
    pub fn sync_data(&mut self, fd: ArcFd) -> &mut Ring {
        self.fsync(fd, types::FsyncFlags::DATASYNC)
    }

    /// Adds an operation that does nothing and completes successfully.
    pub fn nop(&mut self) -> &mut Ring {
        self.push(RingOp::Nop, opcode::Nop::new().build())
    }

    /// Links the last operation to the next one, which then runs only if the
    /// last one succeeded.
    ///
    /// If the last operation fails, the next one and every operation linked
    /// after it complete with an `ECANCELED` error. A read or write that
    /// transfers fewer bytes than requested counts as a failure.
    ///
    /// Does nothing if no operation was added yet. A link from the last
    /// operation of a submission is ignored.
    pub fn then(&mut self) -> &mut Ring {
        self.link(Flags::IO_LINK)
    }

    /// Links the last operation to the next one, which then runs after the
    /// last one even if it failed.
    ///
    /// This only covers the last operation. If an earlier link was broken, the
    /// next one completes with an `ECANCELED` error too.
    ///
    /// Does nothing if no operation was added yet. A link from the last
    /// operation of a submission is ignored.
    pub fn always(&mut self) -> &mut Ring {
        self.link(Flags::IO_HARDLINK)
    }

    /// Calls `add` with each item of `items`, in order, to add its operations.
    ///
    /// The operations don't depend on each other unless `add` links them with
    /// [`then`] or [`always`]. A link from the last operation of an item
    /// reaches the first operation of the next item.
    ///
    /// # Examples
    ///
    /// Read a whole file in parallel chunks:
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use tokio::io::uring::Ring;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// const CHUNK: usize = 64 * 1024;
    ///
    /// let file = Arc::new(std::fs::File::open("data.bin")?);
    /// let len = file.metadata()?.len();
    ///
    /// let done = Ring::new()
    ///     .batch((0..len).step_by(CHUNK), |ring, offset| {
    ///         ring.read(file.clone(), Vec::with_capacity(CHUNK), offset)
    ///     })
    ///     .submit()
    ///     .await;
    ///
    /// let mut contents = Vec::new();
    /// for (read, buf) in done {
    ///     read?;
    ///     contents.extend_from_slice(&buf.unwrap());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`then`]: Ring::then
    /// [`always`]: Ring::always
    pub fn batch<I, F>(&mut self, items: I, mut add: F) -> &mut Ring
    where
        I: IntoIterator,
        F: FnMut(&mut Ring, I::Item) -> &mut Ring,
    {
        let items = items.into_iter();
        // Most callers add one operation per item.
        self.ops.reserve(items.size_hint().0);
        for item in items {
            add(self, item);
        }
        self
    }

    /// Submits the operations added since the last submission, and waits until
    /// all of them completed.
    ///
    /// The operations are submitted to the io_uring instance of the current
    /// runtime when the returned future is first polled. The output has one
    /// completion per operation, in the order the operations were added.
    ///
    /// # Errors
    ///
    /// If the operations can't be submitted, each one completes with the
    /// error, and gives back its buffer unless the kernel may still use it.
    /// This happens if the future is first polled:
    ///
    /// - outside of a Tokio runtime.
    /// - on a runtime without an io_uring instance, which fails with an
    ///   [`Unsupported`] error. For example, the runtime was built without
    ///   [`enable_io_uring`], or the kernel doesn't support io_uring or blocks
    ///   it.
    ///
    /// Operations that are linked must fit in the io_uring submission queue
    /// together, so a submission with links fails with an [`InvalidInput`]
    /// error if it has more operations than the queue has entries.
    ///
    /// [`Unsupported`]: std::io::ErrorKind::Unsupported
    /// [`enable_io_uring`]: crate::runtime::Builder::enable_io_uring
    /// [`InvalidInput`]: std::io::ErrorKind::InvalidInput
    pub fn submit(&mut self) -> Submit {
        Submit {
            ops: mem::take(&mut self.ops),
            state: State::Idle,
        }
    }

    fn fsync(&mut self, fd: ArcFd, flags: types::FsyncFlags) -> &mut Ring {
        let entry = opcode::Fsync::new(types::Fd(fd.as_raw_fd()))
            .flags(flags)
            .build();
        self.push(RingOp::Fsync(fd), entry)
    }

    fn push(&mut self, op: RingOp, entry: Entry) -> &mut Ring {
        self.ops.push(Pending {
            op,
            entry,
            link: Flags::empty(),
        });
        self
    }

    fn link(&mut self, link: Flags) -> &mut Ring {
        if let Some(last) = self.ops.last_mut() {
            last.link = link;
        }
        self
    }
}

impl fmt::Debug for Ring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ring")
            .field("len", &self.ops.len())
            .finish_non_exhaustive()
    }
}

/// An operation of a [`Ring`]. It owns the memory its entry points into.
pub(crate) enum RingOp {
    Read(read::Read<Vec<u8>, ArcFd>),
    Write(write::Write<ArcFd>),
    // The descriptor is only kept open until the op completes, so the field
    // isn't read.
    Fsync(#[allow(dead_code)] ArcFd),
    Nop,
}

impl RingOp {
    /// The completion from the result of the entry.
    fn complete(self, result: io::Result<u32>) -> Completion {
        let cqe = CqeResult { result };
        match self {
            RingOp::Read(read) => {
                let (result, _, buf) = read.complete(cqe);
                (widen(result), buf)
            }
            RingOp::Write(write) => {
                let (result, buf, _) = write.complete(cqe);
                (widen(result), buf.map(into_vec))
            }
            RingOp::Fsync(_) | RingOp::Nop => (widen(cqe.result), None),
        }
    }

    /// The completion when the entry never reached the io_uring, which gives
    /// back the buffer.
    fn complete_with_error(self, error: io::Error) -> Completion {
        match self {
            RingOp::Read(read) => {
                let (result, _, buf) = read.complete_with_error(error);
                (widen(result), buf)
            }
            RingOp::Write(write) => {
                let (result, buf, _) = write.complete_with_error(error);
                (widen(result), buf.map(into_vec))
            }
            RingOp::Fsync(_) | RingOp::Nop => (Err(error), None),
        }
    }
}

impl Cancellable for RingOp {
    fn cancel(self) -> CancelData {
        CancelData::Ring(self)
    }
}

impl fmt::Debug for RingOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RingOp::Read(_) => "Read",
            RingOp::Write(_) => "Write",
            RingOp::Fsync(_) => "Fsync",
            RingOp::Nop => "Nop",
        })
    }
}

fn widen(result: io::Result<u32>) -> io::Result<usize> {
    result.map(|n| n as usize)
}

/// The `Vec` that [`Ring::write`] wrapped.
fn into_vec(buf: OwnedBuf) -> Vec<u8> {
    match buf {
        OwnedBuf::Vec(vec) => vec,
        #[cfg(feature = "io-util")]
        OwnedBuf::Bytes(_) => unreachable!("`Ring::write` only takes a `Vec`"),
    }
}

/// Submits the operations of a [`Ring`] and waits until all of them completed.
///
/// Returned by [`Ring::submit`].
///
/// Dropping it doesn't cancel operations that were already submitted. They
/// keep their buffers and file descriptors until they complete.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Submit {
    /// Empty once the output was returned, or the operations moved into the
    /// driver.
    ops: Vec<Pending>,
    state: State,
}

enum State {
    /// Not submitted yet.
    Idle,
    /// Submitted to the io_uring of `handle`. `slots[i]` tracks the entry of
    /// operation `i`, and only the operations that have a slot were pushed.
    Submitted { handle: Handle, slots: Vec<Slot> },
    Done,
}

/// The entry of a submitted operation.
enum Slot {
    /// In flight at this slab index.
    Pending(usize),
    /// Completed with this result.
    Done(io::Result<u32>),
}

impl Slot {
    fn into_result(self) -> io::Result<u32> {
        match self {
            Slot::Done(res) => res,
            Slot::Pending(_) => unreachable!("every operation completed"),
        }
    }
}

impl Submit {
    /// Pushes the entry of every operation to the io_uring.
    ///
    /// Returns the output if the operations can't be submitted.
    fn submit(&mut self, waker: &Waker) -> Option<Vec<Completion>> {
        if self.ops.is_empty() {
            self.state = State::Done;
            return Some(Vec::new());
        }

        let handle = match uring_handle() {
            Ok(handle) => handle,
            Err(error) => {
                // Nothing was pushed, so every operation gives back its buffer.
                self.state = State::Done;
                let completions = mem::take(&mut self.ops)
                    .into_iter()
                    .map(|pending| pending.op.complete_with_error(copy_error(&error)))
                    .collect();
                return Some(completions);
            }
        };

        let Submit { ops, state } = self;
        let len = ops.len();

        // The slots are part of the state before any entry is pushed, so if
        // this panics midway, `Drop` still keeps the pushed operations alive.
        *state = State::Submitted {
            handle,
            slots: Vec::with_capacity(len),
        };
        let State::Submitted { handle, slots } = state else {
            unreachable!()
        };

        // The last operation has nothing to link to.
        let last = len - 1;
        let linked = ops[..last].iter().any(|pending| !pending.link.is_empty());
        let entries = ops.iter().enumerate().map(|(i, pending)| {
            let link = if i < last {
                pending.link
            } else {
                Flags::empty()
            };
            pending.entry.clone().flags(link)
        });
        let driver = handle.inner.driver().io();

        // SAFETY: every entry points into heap memory owned by its operation
        // (see `Pending`). The operations stay in `self.ops` until their
        // entries complete, or move into the driver with `cancel_op`.
        let result = unsafe {
            driver.register_ops(entries, linked, waker, |index| {
                slots.push(Slot::Pending(index));
            })
        };

        let error = result.err()?;

        // The pushed entries may still reach the kernel, so their operations
        // move into the driver until they complete. The other operations give
        // back their buffers.
        let State::Submitted { handle, slots } = mem::replace(&mut self.state, State::Done) else {
            unreachable!()
        };
        let driver = handle.inner.driver().io();
        let completions = mem::take(&mut self.ops)
            .into_iter()
            .enumerate()
            .map(|(i, pending)| match slots.get(i) {
                Some(&Slot::Pending(index)) => {
                    driver.cancel_op(index, Some(pending.op));
                    (Err(copy_error(&error)), None)
                }
                _ => pending.op.complete_with_error(copy_error(&error)),
            })
            .collect();
        Some(completions)
    }
}

impl Future for Submit {
    type Output = Vec<Completion>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Vec<Completion>> {
        let this = self.get_mut();

        match &mut this.state {
            State::Idle => match this.submit(cx.waker()) {
                Some(output) => Poll::Ready(output),
                None => Poll::Pending,
            },
            State::Submitted { handle, slots } => {
                let driver = handle.inner.driver().io();
                if !poll_slots(driver, slots, cx.waker()) {
                    return Poll::Pending;
                }

                let State::Submitted { slots, .. } = mem::replace(&mut this.state, State::Done) else {
                    unreachable!()
                };
                let completions = mem::take(&mut this.ops)
                    .into_iter()
                    .zip(slots)
                    .map(|(pending, slot)| pending.op.complete(slot.into_result()))
                    .collect();
                Poll::Ready(completions)
            }
            State::Done => panic!("`Submit` polled after completion"),
        }
    }
}

impl Drop for Submit {
    fn drop(&mut self) {
        let State::Submitted { handle, slots } = &self.state else {
            return;
        };

        // The kernel may still use the memory of the operations in flight, so
        // the driver keeps them until they complete. The operations that
        // completed, or were never pushed, are dropped.
        let driver = handle.inner.driver().io();
        for (pending, slot) in mem::take(&mut self.ops).into_iter().zip(slots) {
            if let Slot::Pending(index) = *slot {
                driver.cancel_op(index, Some(pending.op));
            }
        }
    }
}

impl fmt::Debug for Submit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.state {
            State::Idle => "idle",
            State::Submitted { .. } => "submitted",
            State::Done => "done",
        };
        f.debug_struct("Submit")
            .field("len", &self.ops.len())
            .field("state", &state)
            .finish_non_exhaustive()
    }
}

/// Returns the handle of the current runtime, if it has io_uring set up.
fn uring_handle() -> io::Result<Handle> {
    let handle = Handle::try_current().map_err(io::Error::other)?;
    let ready = handle
        .inner
        .driver()
        .io
        .as_ref()
        .is_some_and(|io| io.uring_probe.is_some());

    if !ready {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "io_uring is not available on this runtime",
        ));
    }

    Ok(handle)
}

/// Takes the results of the operations that completed, and makes `waker` the
/// waker of the others. Returns `true` once every operation completed.
fn poll_slots(driver: &driver::Handle, slots: &mut [Slot], waker: &Waker) -> bool {
    let mut replaced = Vec::new();
    let mut done = true;
    let mut ctx = driver.get_uring().lock();

    for slot in slots.iter_mut() {
        let Slot::Pending(index) = *slot else {
            continue;
        };

        match ctx.poll_op(index, waker) {
            PollOp::Ready(cqe) => *slot = Slot::Done(CqeResult::from(cqe).result),
            PollOp::Pending(prev) => {
                done = false;
                replaced.extend(prev);
            }
        }
    }

    // Drop the replaced wakers after the lock is released, as their
    // destructors may lock the driver again.
    drop(ctx);
    drop(replaced);

    done
}

/// Copies `error`, so each operation of a failed submission gets its own.
fn copy_error(error: &io::Error) -> io::Error {
    match error.raw_os_error() {
        Some(code) => io::Error::from_raw_os_error(code),
        None => io::Error::new(error.kind(), error.to_string()),
    }
}
