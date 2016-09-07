use std::cell::RefCell;
use std::io::{self, Read, Write};

use futures::task::TaskRc;

/// The readable half of an object returned from `Io::split`.
pub struct ReadHalf<T> {
    handle: TaskRc<RefCell<T>>,
}

/// The readable half of an object returned from `Io::split`.
pub struct WriteHalf<T> {
    handle: TaskRc<RefCell<T>>,
}

pub fn split<T: Read + Write>(t: T) -> (ReadHalf<T>, WriteHalf<T>) {
    let rc = TaskRc::new(RefCell::new(t));
    (ReadHalf { handle: rc.clone() }, WriteHalf { handle: rc })
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
