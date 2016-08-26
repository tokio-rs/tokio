use std::cell::RefCell;
use std::io::{self, Read, Write};

use futures::task::TaskData;

/// Abstraction that allows inserting an I/O object into task-local storage,
/// returning a handle that can be split.
///
/// A `TaskIo<T>` handle implements the `ReadTask` and `WriteTask` and will only
/// work with the same task that the associated object was inserted into. The
/// handle may then be optionally `split` into the read/write halves so they can
/// be worked with independently.
///
/// Note that it is important that the future returned from `TaskIo::new`, when
/// polled, will pin the yielded `TaskIo<T>` object to that specific task. Any
/// attempt to read or write the object on other tasks will result in a panic.
pub struct TaskIo<T> {
    handle: TaskData<RefCell<T>>,
}

/// The readable half of a `TaskIo<T>` instance returned from `TaskIo::split`.
///
/// This handle implements the `ReadTask` trait and can be used to split up an
/// I/O object into two distinct halves.
pub struct TaskIoRead<T> {
    handle: TaskData<RefCell<T>>,
}

/// The writable half of a `TaskIo<T>` instance returned from `TaskIo::split`.
///
/// This handle implements the `WriteTask` trait and can be used to split up an
/// I/O object into two distinct halves.
pub struct TaskIoWrite<T> {
    handle: TaskData<RefCell<T>>,
}

impl<T> TaskIo<T> {
    /// Returns a new future which represents the insertion of the I/O object
    /// `T` into task local storage, returning a `TaskIo<T>` handle to it.
    ///
    /// The returned future will never resolve to an error.
    pub fn new(t: T) -> TaskIo<T> {
        TaskIo {
            handle: TaskData::new(RefCell::new(t)),
        }
    }
}

impl<T> TaskIo<T>
    where T: Read + Write,
{
    /// For an I/O object which is both readable and writable, this method can
    /// be used to split the handle into two independently owned halves.
    ///
    /// The returned pair implements the `ReadTask` and `WriteTask` traits,
    /// respectively, and can be used to pass around the object to different
    /// combinators if necessary.
    pub fn split(self) -> (TaskIoRead<T>, TaskIoWrite<T>) {
        (TaskIoRead { handle: self.handle.clone() },
         TaskIoWrite { handle: self.handle })
    }
}

impl<T> Read for TaskIo<T>
    where T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().read(buf))
    }
}

impl<T> Write for TaskIo<T>
    where T: io::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle.with(|t| t.borrow_mut().flush())
    }
}

impl<T> Read for TaskIoRead<T>
    where T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().read(buf))
    }
}

impl<T> Write for TaskIoWrite<T>
    where T: io::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle.with(|t| t.borrow_mut().flush())
    }
}
