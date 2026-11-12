use crate::runtime::driver::op::{CancelData, Cancellable, CqeResult, Lifecycle};
use crate::runtime::io::RegisterResult;
use crate::runtime::Handle;

use io_uring::squeue::{Entry, Flags};
use io_uring::{opcode, types};
use std::fmt;
use std::future::{Future, IntoFuture};
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, OwnedFd};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// The offset that makes a read or write use, and advance, the file position.
const CURRENT_POSITION: u64 = u64::MAX;

/// The io_uring instance of a Tokio runtime.
///
/// Describe operations with [`read`], [`write`], [`fsync`] and [`nop`], then
/// submit several of them at once:
///
/// - [`batch`] submits operations that don't depend on each other. They may run
///   in parallel and complete in any order.
/// - [`chain`] links operations, so each one starts only after the previous one
///   completed.
///
/// An operation keeps its buffer and file descriptor until the kernel is done
/// with them, even if the future is dropped before the operation completes.
///
/// [`read`]: Ring::read
/// [`write`]: Ring::write
/// [`fsync`]: Ring::fsync
/// [`nop`]: Ring::nop
/// [`batch`]: Ring::batch
/// [`chain`]: Ring::chain
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
/// let ring = Ring::current()?;
/// let file = Arc::new(std::fs::File::open("data.bin")?);
///
/// let chunks = ring
///     .batch((0..4).map(|i| {
///         ring.read(&file, Vec::with_capacity(CHUNK))
///             .at(i * CHUNK as u64)
///     }))
///     .await;
///
/// for chunk in chunks {
///     println!("read {} bytes", chunk?.len());
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
/// let ring = Ring::current()?;
/// let file = Arc::new(std::fs::File::create("log.txt")?);
///
/// let (written, synced) = ring
///     .chain(ring.write(&file, b"hello\n".to_vec()))
///     .then(ring.fsync(&file))
///     .await;
///
/// written?;
/// synced?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Ring {
    handle: Handle,
}

impl Ring {
    /// Returns the io_uring instance of the current runtime.
    ///
    /// # Errors
    ///
    /// Returns an [`Unsupported`] error if the runtime has no io_uring
    /// instance, for example because it was built without
    /// [`enable_io_uring`], or because the kernel doesn't support io_uring or
    /// blocks it.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a Tokio runtime.
    ///
    /// [`Unsupported`]: std::io::ErrorKind::Unsupported
    /// [`enable_io_uring`]: crate::runtime::Builder::enable_io_uring
    pub fn current() -> io::Result<Ring> {
        let handle = Handle::current();
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

        Ok(Ring { handle })
    }

    /// Reads from `file` into the spare capacity of `buf`.
    ///
    /// The read starts at the file's current position, or at the offset set
    /// with [`Read::at`], and reads at most `buf.capacity() - buf.len()` bytes.
    /// It completes with `buf`, with the bytes read appended.
    pub fn read<F: RingFd>(&self, file: &F, buf: Vec<u8>) -> Read {
        Read {
            fd: file.ring_fd(),
            buf,
            offset: CURRENT_POSITION,
        }
    }

    /// Writes `buf` to `file`.
    ///
    /// The write starts at the file's current position, or at the offset set
    /// with [`Write::at`]. It completes with the number of bytes written, which
    /// may be less than `buf.len()`.
    pub fn write<F: RingFd>(&self, file: &F, buf: Vec<u8>) -> Write {
        Write {
            fd: file.ring_fd(),
            buf,
            offset: CURRENT_POSITION,
        }
    }

    /// Flushes the data and metadata of `file` to disk, like
    /// [`File::sync_all`].
    ///
    /// [`File::sync_all`]: std::fs::File::sync_all
    pub fn fsync<F: RingFd>(&self, file: &F) -> Fsync {
        Fsync {
            fd: file.ring_fd(),
            flags: types::FsyncFlags::empty(),
        }
    }

    /// An operation that does nothing and completes successfully.
    pub fn nop(&self) -> Nop {
        Nop { _private: () }
    }

    /// Submits `ops` together, without linking them.
    ///
    /// The operations don't depend on each other: they may run in parallel and
    /// complete in any order. The output has one result per operation, in the
    /// order of `ops`.
    ///
    /// `ops` can be any iterator, so it can be built with iterator adapters such
    /// as `map` and `filter`.
    pub fn batch<I>(&self, ops: I) -> Submit<Vec<I::Item>>
    where
        I: IntoIterator,
        I::Item: RingOp,
    {
        Submit::new(self.handle.clone(), ops.into_iter().collect(), Vec::new())
    }

    /// Starts a chain with `op`.
    ///
    /// Add operations with [`Chain::then`] and [`Chain::always`], then await
    /// the chain to submit it.
    pub fn chain<O: RingOp>(&self, op: O) -> Chain<(O,)> {
        Chain {
            handle: self.handle.clone(),
            ops: (op,),
            flags: Vec::new(),
        }
    }
}

/// An operation that [`Ring::batch`] and [`Ring::chain`] can submit.
///
/// This trait is sealed. Operations are created by the methods of [`Ring`].
pub trait RingOp: sealed::Op {}

/// A file descriptor that [`Ring`] operations can use.
///
/// An operation keeps a clone of the `Arc`, so the descriptor stays open until
/// the kernel is done with it, even if the future is dropped first.
///
/// This trait is sealed. It is implemented for `Arc<std::fs::File>` and
/// `Arc<OwnedFd>`.
pub trait RingFd: sealed::Fd {}

// Only std types: the driver may drop the last clone while holding its lock,
// and the destructor of a user type could lock the driver again.
impl RingFd for Arc<std::fs::File> {}
impl RingFd for Arc<OwnedFd> {}

impl sealed::Fd for Arc<std::fs::File> {
    fn ring_fd(&self) -> Arc<dyn AsRawFd + Send + Sync> {
        self.clone()
    }
}

impl sealed::Fd for Arc<OwnedFd> {
    fn ring_fd(&self) -> Arc<dyn AsRawFd + Send + Sync> {
        self.clone()
    }
}

/// Reads from a file. Created by [`Ring::read`].
///
/// Completes with `io::Result<Vec<u8>>`: the buffer, with the bytes read
/// appended. If the read fails, the buffer is dropped.
#[must_use = "operations do nothing until submitted with `Ring::batch` or `Ring::chain`"]
pub struct Read {
    fd: Arc<dyn AsRawFd + Send + Sync>,
    buf: Vec<u8>,
    offset: u64,
}

impl Read {
    /// Reads at `offset` instead of the file's current position.
    pub fn at(mut self, offset: u64) -> Read {
        self.offset = offset;
        self
    }
}

impl fmt::Debug for Read {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Read")
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

impl RingOp for Read {}

impl sealed::Op for Read {
    type Output = io::Result<Vec<u8>>;

    fn entry(&mut self) -> Entry {
        let spare = self.buf.spare_capacity_mut();
        let len = u32::try_from(spare.len()).unwrap_or(u32::MAX);
        opcode::Read::new(
            types::Fd(self.fd.as_raw_fd()),
            spare.as_mut_ptr().cast(),
            len,
        )
        .offset(self.offset)
        .build()
    }

    fn complete(mut self, res: io::Result<u32>) -> Self::Output {
        let n = res? as usize;
        // SAFETY: the kernel wrote `n` bytes into the spare capacity, which
        // starts at the old length.
        unsafe { self.buf.set_len(self.buf.len() + n) };
        Ok(self.buf)
    }

    fn failed(error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// Writes to a file. Created by [`Ring::write`].
///
/// Completes with `io::Result<usize>`: the number of bytes written.
#[must_use = "operations do nothing until submitted with `Ring::batch` or `Ring::chain`"]
pub struct Write {
    fd: Arc<dyn AsRawFd + Send + Sync>,
    buf: Vec<u8>,
    offset: u64,
}

impl Write {
    /// Writes at `offset` instead of the file's current position.
    pub fn at(mut self, offset: u64) -> Write {
        self.offset = offset;
        self
    }
}

impl fmt::Debug for Write {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Write")
            .field("len", &self.buf.len())
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

impl RingOp for Write {}

impl sealed::Op for Write {
    type Output = io::Result<usize>;

    fn entry(&mut self) -> Entry {
        let len = u32::try_from(self.buf.len()).unwrap_or(u32::MAX);
        opcode::Write::new(types::Fd(self.fd.as_raw_fd()), self.buf.as_ptr(), len)
            .offset(self.offset)
            .build()
    }

    fn complete(self, res: io::Result<u32>) -> Self::Output {
        res.map(|n| n as usize)
    }

    fn failed(error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// Flushes a file to disk. Created by [`Ring::fsync`].
///
/// Completes with `io::Result<()>`.
#[must_use = "operations do nothing until submitted with `Ring::batch` or `Ring::chain`"]
pub struct Fsync {
    fd: Arc<dyn AsRawFd + Send + Sync>,
    flags: types::FsyncFlags,
}

impl Fsync {
    /// Flushes only the data, and the metadata needed to read it back, like
    /// [`File::sync_data`].
    ///
    /// [`File::sync_data`]: std::fs::File::sync_data
    pub fn data_only(mut self) -> Fsync {
        self.flags = types::FsyncFlags::DATASYNC;
        self
    }
}

impl fmt::Debug for Fsync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fsync")
            .field(
                "data_only",
                &self.flags.contains(types::FsyncFlags::DATASYNC),
            )
            .finish_non_exhaustive()
    }
}

impl RingOp for Fsync {}

impl sealed::Op for Fsync {
    type Output = io::Result<()>;

    fn entry(&mut self) -> Entry {
        opcode::Fsync::new(types::Fd(self.fd.as_raw_fd()))
            .flags(self.flags)
            .build()
    }

    fn complete(self, res: io::Result<u32>) -> Self::Output {
        res.map(drop)
    }

    fn failed(error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// Does nothing. Created by [`Ring::nop`].
///
/// Completes with `io::Result<()>`.
#[derive(Debug)]
#[must_use = "operations do nothing until submitted with `Ring::batch` or `Ring::chain`"]
pub struct Nop {
    _private: (),
}

impl RingOp for Nop {}

impl sealed::Op for Nop {
    type Output = io::Result<()>;

    fn entry(&mut self) -> Entry {
        opcode::Nop::new().build()
    }

    fn complete(self, res: io::Result<u32>) -> Self::Output {
        res.map(drop)
    }

    fn failed(error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// Operations that run one after another. Created by [`Ring::chain`].
///
/// Each operation starts only after the previous one completed. Awaiting the
/// chain submits it. The output is a tuple with one result per operation, in
/// order. A chain holds up to 8 operations.
#[must_use = "a chain does nothing unless awaited"]
pub struct Chain<L> {
    handle: Handle,
    ops: L,
    /// `flags[i]` links operation `i` to operation `i + 1`.
    flags: Vec<Flags>,
}

impl<L> Chain<L> {
    /// Adds `op`, which runs only if the previous operation succeeded.
    ///
    /// If the previous operation fails, `op` and every operation after it
    /// complete with an `ECANCELED` error. A read or write that transfers fewer
    /// bytes than requested counts as a failure.
    pub fn then<O: RingOp>(self, op: O) -> Chain<L::Out>
    where
        L: sealed::Append<O>,
    {
        self.push(op, Flags::IO_LINK)
    }

    /// Adds `op`, which runs after the previous operation even if it failed.
    ///
    /// This only covers the previous operation. If an earlier link was broken,
    /// `op` completes with an `ECANCELED` error too.
    pub fn always<O: RingOp>(self, op: O) -> Chain<L::Out>
    where
        L: sealed::Append<O>,
    {
        self.push(op, Flags::IO_HARDLINK)
    }

    fn push<O>(self, op: O, link: Flags) -> Chain<L::Out>
    where
        L: sealed::Append<O>,
    {
        let mut flags = self.flags;
        flags.push(link);
        Chain {
            handle: self.handle,
            ops: self.ops.append(op),
            flags,
        }
    }
}

impl<L> fmt::Debug for Chain<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chain")
            .field("len", &(self.flags.len() + 1))
            .finish_non_exhaustive()
    }
}

impl<L: sealed::Ops> IntoFuture for Chain<L> {
    type Output = L::Output;
    type IntoFuture = Submit<L>;

    fn into_future(self) -> Submit<L> {
        Submit::new(self.handle, self.ops, self.flags)
    }
}

/// Submits operations and waits until all of them completed.
///
/// Returned by [`Ring::batch`], and by awaiting a [`Chain`].
///
/// Dropping it doesn't cancel operations that were already submitted. They
/// keep their buffers and file descriptors until they complete.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Submit<L: sealed::Ops> {
    handle: Handle,
    /// `None` once the output was returned, or the operations moved into the
    /// driver.
    ops: Option<L>,
    state: State,
}

enum State {
    /// Not submitted yet. `flags[i]` links operation `i` to the next one.
    Idle(Vec<Flags>),
    /// Submitted. `results[i]` is set once the entry of operation `i` completed.
    Submitted {
        index: Vec<usize>,
        results: Vec<Option<io::Result<u32>>>,
    },
    Done,
}

impl<L: sealed::Ops> Submit<L> {
    fn new(handle: Handle, ops: L, flags: Vec<Flags>) -> Submit<L> {
        Submit {
            handle,
            ops: Some(ops),
            state: State::Idle(flags),
        }
    }

    /// Pushes the entries of every operation to the io_uring.
    ///
    /// Returns the output if the operations can't be submitted.
    fn submit(&mut self, flags: &[Flags], cx: &Context<'_>) -> Option<L::Output> {
        let ops = self
            .ops
            .as_mut()
            .expect("operations are present until completion");
        let entries: Vec<Entry> = ops
            .entries()
            .into_iter()
            .enumerate()
            .map(|(i, entry)| match flags.get(i) {
                Some(&link) => entry.flags(link),
                None => entry,
            })
            .collect();

        if entries.is_empty() {
            self.state = State::Done;
            return self.ops.take().map(|ops| ops.complete(Vec::new()));
        }

        let driver = self.handle.inner.driver().io();
        // SAFETY: every entry points into heap memory owned by its operation
        // (see `sealed::Op::entry`). The operations stay in `self.ops` until
        // their entries complete, or move into the driver with `cancel_op`.
        let result = unsafe { driver.register_ops(&entries, !flags.is_empty(), cx.waker()) };

        match result {
            RegisterResult::Submitted(index) => {
                let results = index.iter().map(|_| None).collect();
                self.state = State::Submitted { index, results };
                None
            }
            RegisterResult::NotPushed(error) => {
                self.ops = None;
                self.state = State::Done;
                Some(L::failed(entries.len(), &error))
            }
            RegisterResult::Pushed { index, error } => {
                // The pushed entries may still reach the kernel, so their
                // operations move into the driver until they complete.
                let ops = self
                    .ops
                    .take()
                    .expect("operations are present until completion");
                for (index, keep) in index.into_iter().zip(ops.keep()) {
                    driver.cancel_op(index, Some(keep));
                }
                self.state = State::Done;
                Some(L::failed(entries.len(), &error))
            }
        }
    }
}

impl<L: sealed::Ops> Future for Submit<L> {
    type Output = L::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<L::Output> {
        let this = self.get_mut();

        match &mut this.state {
            State::Idle(flags) => {
                let flags = mem::take(flags);
                match this.submit(&flags, cx) {
                    Some(output) => Poll::Ready(output),
                    None => Poll::Pending,
                }
            }
            State::Submitted { index, results } => {
                if !poll_results(&this.handle, index, results, cx) {
                    return Poll::Pending;
                }

                let results: Vec<_> = results
                    .iter_mut()
                    .map(|res| res.take().expect("every operation completed"))
                    .collect();
                this.state = State::Done;
                let ops = this
                    .ops
                    .take()
                    .expect("operations are present until completion");
                Poll::Ready(ops.complete(results))
            }
            State::Done => panic!("`Submit` polled after completion"),
        }
    }
}

impl<L: sealed::Ops> Unpin for Submit<L> {}

impl<L: sealed::Ops> Drop for Submit<L> {
    fn drop(&mut self) {
        let State::Submitted { index, results } = &self.state else {
            return;
        };
        let Some(ops) = self.ops.take() else {
            return;
        };

        let driver = self.handle.inner.driver().io();
        for ((&index, res), keep) in index.iter().zip(results).zip(ops.keep()) {
            if res.is_none() {
                // The kernel may still use the memory of this operation, so the
                // driver keeps it until the operation completes.
                driver.cancel_op(index, Some(keep));
            }
        }
    }
}

impl<L: sealed::Ops> fmt::Debug for Submit<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.state {
            State::Idle(_) => "idle",
            State::Submitted { .. } => "submitted",
            State::Done => "done",
        };
        f.debug_struct("Submit")
            .field("state", &state)
            .finish_non_exhaustive()
    }
}

/// Takes the results of the operations that completed, and stores the waker
/// of `cx` for the others. Returns `true` once every operation completed.
fn poll_results(
    handle: &Handle,
    index: &[usize],
    results: &mut [Option<io::Result<u32>>],
    cx: &Context<'_>,
) -> bool {
    let driver = handle.inner.driver().io();
    let mut replaced = Vec::new();
    let mut ctx = driver.get_uring().lock();

    for (&index, res) in index.iter().zip(results.iter_mut()) {
        if res.is_some() {
            continue;
        }

        let lifecycle = ctx.ops.get_mut(index).expect("Lifecycle must be present");
        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Completed(cqe) => {
                ctx.remove_op(index);
                *res = Some(CqeResult::from(cqe).result);
            }
            Lifecycle::Waiting(prev) if !prev.will_wake(cx.waker()) => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                replaced.push(prev);
            }
            waiting @ Lifecycle::Waiting(_) => *lifecycle = waiting,
            Lifecycle::Submitted | Lifecycle::Cancelled(_) => {
                unreachable!("an operation that is awaited is waiting or completed")
            }
        }
    }

    // Drop the replaced wakers after the lock is released, as their
    // destructors may lock the driver again.
    drop(ctx);
    drop(replaced);

    results.iter().all(Option::is_some)
}

/// Copies `error`, so each operation of a failed submission gets its own.
fn copy_error(error: &io::Error) -> io::Error {
    match error.raw_os_error() {
        Some(code) => io::Error::from_raw_os_error(code),
        None => io::Error::new(error.kind(), error.to_string()),
    }
}

impl<O: RingOp> sealed::Ops for Vec<O> {
    type Output = Vec<O::Output>;

    fn entries(&mut self) -> Vec<Entry> {
        self.iter_mut().map(sealed::Op::entry).collect()
    }

    fn complete(self, results: Vec<io::Result<u32>>) -> Self::Output {
        self.into_iter()
            .zip(results)
            .map(|(op, res)| op.complete(res))
            .collect()
    }

    fn failed(count: usize, error: &io::Error) -> Self::Output {
        (0..count).map(|_| O::failed(copy_error(error))).collect()
    }

    fn keep(self) -> Vec<sealed::Keep> {
        self.into_iter().map(sealed::Keep::new).collect()
    }
}

macro_rules! impl_ops {
    ($($op:ident $i:tt),+) => {
        impl<$($op: RingOp),+> sealed::Ops for ($($op,)+) {
            type Output = ($($op::Output,)+);

            fn entries(&mut self) -> Vec<Entry> {
                vec![$(self.$i.entry()),+]
            }

            fn complete(self, results: Vec<io::Result<u32>>) -> Self::Output {
                let mut results = results.into_iter();
                ($(self.$i.complete(results.next().expect("one result per operation")),)+)
            }

            fn failed(_count: usize, error: &io::Error) -> Self::Output {
                ($($op::failed(copy_error(error)),)+)
            }

            fn keep(self) -> Vec<sealed::Keep> {
                vec![$(sealed::Keep::new(self.$i)),+]
            }
        }
    };
}

impl_ops!(A 0);
impl_ops!(A 0, B 1);
impl_ops!(A 0, B 1, C 2);
impl_ops!(A 0, B 1, C 2, D 3);
impl_ops!(A 0, B 1, C 2, D 3, E 4);
impl_ops!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_ops!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
impl_ops!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);

macro_rules! impl_append {
    ($($op:ident $i:tt),+) => {
        impl<$($op: RingOp,)+ Next: RingOp> sealed::Append<Next> for ($($op,)+) {
            type Out = ($($op,)+ Next,);

            fn append(self, op: Next) -> Self::Out {
                ($(self.$i,)+ op,)
            }
        }
    };
}

impl_append!(A 0);
impl_append!(A 0, B 1);
impl_append!(A 0, B 1, C 2);
impl_append!(A 0, B 1, C 2, D 3);
impl_append!(A 0, B 1, C 2, D 3, E 4);
impl_append!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_append!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);

pub(crate) mod sealed {
    //! The internals of the ring API. They are not part of its public
    //! interface.

    use super::*;

    /// One io_uring operation.
    pub trait Op: Send + 'static {
        /// The value the operation completes with.
        type Output;

        /// Builds the entry of the operation.
        ///
        /// Every pointer in the entry points into heap memory owned by `self`,
        /// so it stays valid when `self` is moved, until `self` is dropped.
        fn entry(&mut self) -> Entry;

        /// Builds the output from the result of the entry.
        fn complete(self, res: io::Result<u32>) -> Self::Output;

        /// The output when the operation could not be submitted.
        fn failed(error: io::Error) -> Self::Output;
    }

    /// The operations of one batch or chain, in order.
    pub trait Ops: Send + 'static {
        /// One output per operation.
        type Output;

        /// One entry per operation.
        fn entries(&mut self) -> Vec<Entry>;

        /// Builds the outputs from one result per operation.
        fn complete(self, results: Vec<io::Result<u32>>) -> Self::Output;

        /// The outputs when `count` operations could not be submitted.
        fn failed(count: usize, error: &io::Error) -> Self::Output;

        /// Splits the operations, to keep the memory of each one alive.
        fn keep(self) -> Vec<Keep>;
    }

    /// Adds an operation to the end of a chain.
    pub trait Append<O> {
        /// The operations of the longer chain.
        type Out: Ops;

        /// Adds `op` after the other operations.
        fn append(self, op: O) -> Self::Out;
    }

    /// A file descriptor that operations keep open until they complete.
    pub trait Fd {
        /// A clone that keeps the descriptor open.
        fn ring_fd(&self) -> Arc<dyn AsRawFd + Send + Sync>;
    }

    /// An operation whose entry may still be used by the kernel.
    pub struct Keep(#[allow(dead_code)] Box<dyn Send>);

    impl Keep {
        pub(super) fn new<O: Op>(op: O) -> Keep {
            Keep(Box::new(op))
        }
    }

    impl fmt::Debug for Keep {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Keep")
        }
    }

    impl Cancellable for Keep {
        fn cancel(self) -> CancelData {
            CancelData::Ring(self)
        }
    }
}
