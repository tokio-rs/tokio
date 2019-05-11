use super::File;
use crate::blocking_pool::{blocking, Blocking};
use futures::future;
use futures::{Async, Future, Poll};
use std::fs::{File as StdFile, Metadata, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::mem;
use std::sync::{Arc, Mutex};

/// The fallback async file implementation.
///
/// This implementation spawns blocking operations onto the blocking pool instead of relying on
/// `tokio_threadpool::blocking()`.
#[derive(Debug)]
pub struct Fallback {
    /// The status of the file (idle or blocked on an async operation).
    ///
    /// The status is wrapped in an `Arc<Mutex<_>>` because file handles can be cloned and shared
    /// among threads. Operating systems similarly have a lock around their internal file states.
    status: Arc<Mutex<Status>>,
}

/// The status of a file.
#[derive(Debug)]
enum Status {
    /// The file is idle and its state is readable.
    ///
    /// The state is set to `None` when the file is closed.
    Idle(Option<State>),

    /// The file is blocked on an async operation.
    Blocked(Job),
}

/// The file state.
#[derive(Debug)]
struct State {
    /// The file handle.
    std: StdFile,

    /// The buffer for reading or writing.
    buffer: Buffer,

    /// The current buffer kind (reader or writer).
    kind: BufferKind,

    /// The result of the last async operation.
    last_op: Option<LastOp>,
}

/// The current buffer kind.
#[derive(Debug, Eq, PartialEq)]
enum BufferKind {
    /// The buffer is used for reading bytes from the file.
    Reader,

    /// The buffer is used for writing bytes into the file.
    Writer,
}

/// The result of the last async operation.
#[derive(Debug)]
enum LastOp {
    /// A `seek` operation with the `io::SeekFrom` argument can succeed with the `u64` result.
    Preseek(io::SeekFrom, u64),

    /// A `flush` operation has completed.
    Flush,

    /// A `sync_all` operation has completed.
    SyncAll,

    /// A `sync_data` operation has completed.
    SyncData,

    /// A `set_len` operation with the `u64` argument has completed.
    SetLen(u64),

    /// A `metadata` operation has fetched the file metadata.
    Metadata(Metadata),

    /// A `set_permissions` operation with the `Permissions` argument has completed.
    SetPermissions(Permissions),

    /// A read operation has reached the end of the file.
    EndOfFile,
}

/// A result of the closure passed to `Fallback::update()`.
///
/// This result indicates how the update loop should complete or continue.
#[derive(Debug)]
enum Update<T> {
    /// The loop completes with a new state and a result.
    Done(Option<State>, T),

    /// The loop completes with a new job and a result.
    Prefetch(Job, T),

    /// The loop continues with a new job.
    Block(Job),
}

impl Fallback {
    /// Creates a new `Fallback` from a `std::fs::File`.
    pub fn new(std: StdFile) -> Fallback {
        Fallback {
            status: Arc::new(Mutex::new(Status::Idle(Some(State {
                std,
                buffer: Buffer::new(),
                kind: BufferKind::Reader,
                last_op: None,
            })))),
        }
    }

    /// Seek to an offset.
    pub fn poll_seek(&mut self, pos: io::SeekFrom) -> Poll<u64, io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                // NOTE(stjepang): This is a hack.
                //
                // There are two naive ways of implementing this function:
                //
                // 1. Schedule a seek operation and return `Ready` immediately. But we also have to
                //    return a value of type `u64`, which is the position of the cursor after the
                //    seek operation. Unfortunately, we don't know whether the seek operation will
                //    result in an error so we'd have to somehow track the position of the cursor.
                //
                // 2. Schedule a seek operation and return `NotReady`. Let the user keep polling
                //    until the operation completes and then return the result of it. But now we
                //    run into another problem. We'd like for an async operation to not move the
                //    cursor unless `Ready` is returned. Put differently, we want the seek
                //    operation to happen "atomically" at the moment `Ready` is returned.
                //
                // This brings us to a hacky solution that works okay, although it is not the
                // nicest or the fastest one. Perhaps we'll figure out a better one in the future.
                //
                // First we schedule a "pre-seek" operation that only figures out the current and
                // tfinal positions of the cursor, but doesn't really move it. We return an error
                // early if the seek operation would fail.
                //
                // Finally, after the pre-seek operation completes, we can schedule an actual seek
                // operation (which we know will succeed!) and return `Ready` immediately with the
                // known final position of the cursor.
                if let Some(LastOp::Preseek(last_pos, output)) = state.last_op.take() {
                    if last_pos == pos {
                        return Update::Prefetch(Job::seek(state, pos), output);
                    }
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else if state.kind == BufferKind::Reader && !state.buffer.is_empty() {
                    Update::Block(Job::unread(state))
                } else {
                    Update::Block(Job::preseek(state, pos))
                }
            }
        })
    }

    /// Attempts to sync all OS-internal metadata to disk.
    pub fn poll_sync_all(&mut self) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::SyncAll) = state.last_op.take() {
                    return Update::Done(Some(state), ());
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else {
                    Update::Block(Job::sync_all(state))
                }
            }
        })
    }

    /// Attempts to sync OS-internal buffered data to disk.
    pub fn poll_sync_data(&mut self) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::SyncData) = state.last_op.take() {
                    return Update::Done(Some(state), ());
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else {
                    Update::Block(Job::sync_data(state))
                }
            }
        })
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    pub fn poll_set_len(&mut self, size: u64) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::SetLen(last_size)) = state.last_op.take() {
                    if last_size == size {
                        return Update::Done(Some(state), ());
                    }
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else if state.kind == BufferKind::Reader && !state.buffer.is_empty() {
                    Update::Block(Job::unread(state))
                } else {
                    Update::Block(Job::set_len(state, size))
                }
            }
        })
    }

    /// Queries metadata about the underlying file.
    pub fn poll_metadata(&mut self) -> Poll<Metadata, io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::Metadata(metadata)) = state.last_op.take() {
                    return Update::Done(Some(state), metadata);
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else {
                    Update::Block(Job::metadata(state))
                }
            }
        })
    }

    /// Create a new `File` instance that shares the same underlying file handle.
    pub fn poll_try_clone(&mut self) -> Poll<File, io::Error> {
        let fallback = Fallback {
            status: self.status.clone(),
        };
        Ok(File::from_fallback(fallback).into())
    }

    /// Changes the permissions on the underlying file.
    pub fn poll_set_permissions(&mut self, perms: Permissions) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::SetPermissions(last_perms)) = state.last_op.take() {
                    if last_perms == perms {
                        return Update::Done(Some(state), ());
                    }
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else if state.kind == BufferKind::Reader && !state.buffer.is_empty() {
                    Update::Block(Job::unread(state))
                } else {
                    let perms = perms.clone();
                    Update::Block(Job::set_permissions(state, perms))
                }
            }
        })
    }

    /// Read some bytes into the specified buffer, returning how many bytes were read.
    pub fn poll_read(&mut self, buf: &mut [u8]) -> Poll<usize, io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::EndOfFile) = state.last_op.take() {
                    if state.kind == BufferKind::Reader {
                        let n = state.buffer.read(buf);
                        return Update::Done(Some(state), n);
                    }
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else if state.buffer.is_empty() {
                    Update::Block(Job::read(state))
                } else {
                    let n = state.buffer.read(buf);

                    if n > 0 && state.buffer.is_empty() {
                        Update::Prefetch(Job::read(state), n)
                    } else {
                        Update::Done(Some(state), n)
                    }
                }
            }
        })
    }

    /// Write some bytes into this file, returning how many bytes were written.
    pub fn poll_write(&mut self, buf: &[u8]) -> Poll<usize, io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                state.last_op.take();

                if state.kind == BufferKind::Reader && !state.buffer.is_empty() {
                    Update::Block(Job::unread(state))
                } else if state.buffer.is_full() {
                    Update::Block(Job::write(state))
                } else {
                    let n = state.buffer.write(buf);
                    state.kind = BufferKind::Writer;
                    Update::Done(Some(state), n)
                }
            }
        })
    }

    /// Flushes the write buffer.
    pub fn poll_flush(&mut self) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => panic!("`File` instance closed"),
            Some(mut state) => {
                if let Some(LastOp::Flush) = state.last_op.take() {
                    return Update::Done(Some(state), ());
                }

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else {
                    Update::Block(Job::flush(state))
                }
            }
        })
    }

    /// Flushes the write buffer and closes the file.
    pub fn poll_close(&mut self) -> Poll<(), io::Error> {
        self.update(|idle| match idle {
            None => Update::Done(None, ()),
            Some(mut state) => {
                state.last_op.take();

                if state.kind == BufferKind::Writer && !state.buffer.is_empty() {
                    Update::Block(Job::write(state))
                } else {
                    Update::Block(Job::close(state))
                }
            }
        })
    }

    /// Destructures the `Fallback` into a `std::fs::File`.
    pub fn into_std(self) -> StdFile {
        match Arc::try_unwrap(self.status)
            .expect("`File` instance is not the only reference to the underlying `std::fs::File`")
            .into_inner()
            .unwrap_or_else(|err| err.into_inner())
        {
            Status::Idle(Some(State { std, .. })) => std,
            Status::Blocked(..) => panic!("`File` instance blocked on an async operation"),
            Status::Idle(None) => panic!("`File` instance closed"),
        }
    }

    /// Updates the file state in a loop with a function that operates on idle state.
    ///
    /// If the file is blocked on a job, the job is polled. If the file is idle, the provided
    /// function is invoked on the state and then one of the following happens:
    ///
    /// 1. The loop completes with a new state and a result of type `T`.
    /// 2. The loop completes with a new job and a result of type `T`.
    /// 3. The loop continues with a new job.
    fn update<F, T>(&mut self, mut f: F) -> Poll<T, io::Error>
    where
        F: FnMut(Option<State>) -> Update<T>,
    {
        let status = &mut *self.status.lock().unwrap_or_else(|err| err.into_inner());
        loop {
            match mem::replace(status, Status::Idle(None)) {
                Status::Idle(idle) => match f(idle) {
                    Update::Done(idle, res) => {
                        *status = Status::Idle(idle);
                        return Ok(res.into());
                    }
                    Update::Prefetch(mut job, res) => {
                        // Poll once in order to start the job.
                        assert!(job.0.poll().unwrap().is_not_ready());
                        *status = Status::Blocked(job);
                        return Ok(res.into());
                    }
                    Update::Block(job) => {
                        *status = Status::Blocked(job);
                    }
                },
                Status::Blocked(mut job) => match job.0.poll() {
                    Ok(Async::Ready(s)) => {
                        *status = Status::Idle(s);
                    }
                    Ok(Async::NotReady) => {
                        *status = Status::Blocked(job);
                        return Ok(Async::NotReady);
                    }
                    Err((idle, err)) => {
                        *status = Status::Idle(idle);
                        return Err(err);
                    }
                },
            }
        }
    }
}

/// An async blocking job that returns back the updated file state when finished.
#[derive(Debug)]
struct Job(Blocking<Option<State>, (Option<State>, io::Error)>);

impl Job {
    fn new<F>(f: F) -> Job
    where
        F: FnOnce() -> (Option<State>, io::Result<()>) + Send + 'static,
    {
        Job(blocking(future::lazy(move || match f() {
            (idle, Ok(())) => Ok(idle),
            (idle, Err(err)) => Err((idle, err)),
        })))
    }

    /// Creates a job that prepares a seek operation.
    fn preseek(mut state: State, pos: io::SeekFrom) -> Job {
        Job::new(move || {
            // Find out the current position in the file.
            let curr = match state.std.seek(io::SeekFrom::Current(0)) {
                Ok(curr) => curr,
                Err(err) => return (Some(state), Err(err)),
            };
            // Try executing a seek operation with `pos` as the argument.
            let res = state.std.seek(pos).map(|output| {
                // If successful, store the result of the seek operation.
                state.last_op = Some(LastOp::Preseek(pos, output));
                // Undo the seek operation to return to the original position.
                state.std.seek(io::SeekFrom::Start(curr)).unwrap();
            });
            (Some(state), res)
        })
    }

    /// Creates a job that seeks to an offset.
    fn seek(mut state: State, pos: io::SeekFrom) -> Job {
        Job::new(move || {
            // This operation must succeed because a preseek operation has ensured so.
            let res = state.std.seek(pos).map(|_| ());
            (Some(state), res)
        })
    }

    /// Creates a job that syncs all OS-internal metadata to disk.
    fn sync_all(mut state: State) -> Job {
        Job::new(move || {
            let res = state
                .std
                .sync_all()
                .map(|_| state.last_op = Some(LastOp::SyncAll));
            (Some(state), res)
        })
    }

    /// Creates a job that syncs OS-internal buffered data to disk.
    fn sync_data(mut state: State) -> Job {
        Job::new(move || {
            let res = state
                .std
                .sync_data()
                .map(|_| state.last_op = Some(LastOp::SyncData));
            (Some(state), res)
        })
    }

    /// Creates a job that resizes the file.
    fn set_len(mut state: State, size: u64) -> Job {
        Job::new(move || {
            let res = state
                .std
                .set_len(size)
                .map(|_| state.last_op = Some(LastOp::SetLen(size)));
            (Some(state), res)
        })
    }

    /// Creates a job that fetches the file metadata.
    fn metadata(mut state: State) -> Job {
        Job::new(move || {
            let res = state
                .std
                .metadata()
                .map(|metadata| state.last_op = Some(LastOp::Metadata(metadata)));
            (Some(state), res)
        })
    }

    /// Creates a job that changes the permissions on the file.
    fn set_permissions(mut state: State, perms: Permissions) -> Job {
        Job::new(move || {
            let res = state
                .std
                .set_permissions(perms.clone())
                .map(|_| state.last_op = Some(LastOp::SetPermissions(perms)));
            (Some(state), res)
        })
    }

    /// Creates a job that reads bytes from the file to refill the buffer.
    fn read(mut state: State) -> Job {
        Job::new(move || {
            let res = state.buffer.import(&mut state.std).map(|n| {
                if n == 0 {
                    state.last_op = Some(LastOp::EndOfFile);
                }
            });
            state.kind = BufferKind::Reader;
            (Some(state), res)
        })
    }

    /// Creates a job that clears the read buffer and seeks back to where the cursor was before
    /// reading.
    fn unread(mut state: State) -> Job {
        Job::new(move || {
            let offset = state.buffer.len() as i64;
            let res = state
                .std
                .seek(SeekFrom::Current(-offset))
                .map(|_| state.buffer.clear());
            (Some(state), res)
        })
    }

    /// Creates a job that pulls some bytes from the buffer and writes them into the file.
    fn write(mut state: State) -> Job {
        Job::new(move || {
            let res = state.buffer.export(&mut state.std).map(drop);
            (Some(state), res)
        })
    }

    /// Creates a job that flushes the OS-internal write buffer.
    fn flush(mut state: State) -> Job {
        Job::new(move || {
            let res = state
                .std
                .flush()
                .map(|_| state.last_op = Some(LastOp::Flush));
            (Some(state), res)
        })
    }

    /// Creates a job that closes the file.
    fn close(state: State) -> Job {
        Job::new(move || {
            drop(state.std);
            (None, Ok(()))
        })
    }
}

/// A byte buffer for reading/writing from/into a file.
///
/// The buffer contains a storage vector and numbers `pos` and `len`. The actual bytes inside the
/// buffer are kept in slice `bytes[pos..pos + len]`. The slice is always continuous - i.e. it
/// never wraps around past the end of the vector.
#[derive(Debug)]
struct Buffer {
    /// Storage for the buffer.
    bytes: Vec<u8>,
    /// The start index into the storage vector.
    pos: usize,
    /// The number of bytes in the buffer.
    len: usize,
}

impl Buffer {
    /// Creates a new byte buffer.
    fn new() -> Buffer {
        Buffer {
            bytes: vec![],
            pos: 0,
            len: 0,
        }
    }

    /// Returns `true` if the buffer is empty.
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the buffer is full.
    fn is_full(&self) -> bool {
        self.len == self.bytes.len()
    }

    /// Returns the number of bytes in the buffer.
    fn len(&self) -> usize {
        self.len
    }

    /// Clears the buffer.
    fn clear(&mut self) {
        self.pos = 0;
        self.len = 0;
    }

    /// Reads some bytes into `buf` and returns the number of bytes transferred.
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let count = buf.len().min(self.len).min(self.bytes.len() - self.pos);
        buf[0..count].copy_from_slice(&self.bytes[self.pos..self.pos + count]);

        self.pos += count;
        self.len -= count;
        if self.pos == self.bytes.len() || self.len == 0 {
            self.pos = 0;
        }

        count
    }

    /// Writes some bytes from `buf` and returns the number of bytes transferred.
    fn write(&mut self, buf: &[u8]) -> usize {
        self.init();

        let mut start = self.pos + self.len;
        if start >= self.bytes.len() {
            start -= self.bytes.len();
        }

        let count = buf
            .len()
            .min(self.bytes.len() - self.len)
            .min(self.bytes.len() - start);
        self.bytes[self.len..self.len + count].copy_from_slice(&buf[0..count]);

        self.len += count;
        count
    }

    /// Export some bytes into `writer`.
    ///
    /// On success, returns the number of bytes transferred.
    ///
    /// Note that this is a blocking operation.
    fn export<W: Write>(&mut self, mut writer: W) -> io::Result<usize> {
        self.init();

        let start = self.pos;
        let count = self.len.min(self.bytes.len() - start);
        let res = writer.write(&self.bytes[start..start + count]);

        if let Ok(n) = res {
            self.len -= n;
            self.pos += n;
            if self.pos == self.bytes.len() || self.len == 0 {
                self.pos = 0;
            }
        }

        res
    }

    /// Import some bytes from `reader`.
    ///
    /// On success, eturns the number of bytes transferred.
    ///
    /// Note that this is a blocking operation.
    fn import<R: Read>(&mut self, mut reader: R) -> io::Result<usize> {
        self.init();

        let mut start = self.pos + self.len;
        if start >= self.bytes.len() {
            start -= self.bytes.len();
        }

        let count = (self.bytes.len() - self.len).min(self.bytes.len() - start);
        let res = reader.read(&mut self.bytes[start..start + count]);

        if let Ok(n) = res {
            self.len += n;
        }

        res
    }

    /// Initializes the internal byte vector.
    fn init(&mut self) {
        const LEN: usize = 8 * 1024; // 8 kb

        if self.bytes.len() == 0 {
            // TODO(stjepang): Maybe we should initially start with a small vector and grow it if
            // the file turns out to be large. If we're only operating on tiny files, allocations
            // of 4 kB are probably wasteful.
            self.bytes = vec![0u8; LEN];
        }
    }
}
