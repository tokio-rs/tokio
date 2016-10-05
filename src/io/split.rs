use std::cell::RefCell;
use std::io::{self, Read, Write};

use futures::Async;
use futures::task::TaskRc;

use io::Io;

/// The readable half of an object returned from `Io::split`.
pub struct ReadHalf<T> {
    handle: TaskRc<RefCell<T>>,
}

/// The writable half of an object returned from `Io::split`.
pub struct WriteHalf<T> {
    handle: TaskRc<RefCell<T>>,
}

pub fn split<T: Io>(t: T) -> (ReadHalf<T>, WriteHalf<T>) {
    let rc = TaskRc::new(RefCell::new(t));
    (ReadHalf { handle: rc.clone() }, WriteHalf { handle: rc })
}

impl<T: Io> ReadHalf<T> {
    /// Calls the underlying `poll_read` function on this handling, testing to
    /// see if it's ready to be read from.
    pub fn poll_read(&mut self) -> Async<()> {
        self.handle.with(|t| t.borrow_mut().poll_read())
    }
}

impl<T: Io> WriteHalf<T> {
    /// Calls the underlying `poll_write` function on this handling, testing to
    /// see if it's ready to be written to.
    pub fn poll_write(&mut self) -> Async<()> {
        self.handle.with(|t| t.borrow_mut().poll_write())
    }
}

impl<T: Read> Read for ReadHalf<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().read(buf))
    }
}

impl<T: Write> Write for WriteHalf<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.with(|t| t.borrow_mut().write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle.with(|t| t.borrow_mut().flush())
    }
}
