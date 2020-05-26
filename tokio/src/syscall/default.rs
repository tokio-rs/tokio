//! Default [Syscalls]
use super::Syscalls;
use crate::park::Either;
use crate::park::Park;
use std::io;
use std::sync::Mutex;

pub(crate) struct DefaultSyscalls {
    inner: Mutex<Inner>,
}

struct Inner {
    io_driver: crate::runtime::time::Driver,
}

impl DefaultSyscalls {
    pub(crate) fn new(io_driver: crate::runtime::time::Driver) -> Self {
        Self {
            inner: Mutex::new(Inner { io_driver }),
        }
    }
}

impl Syscalls for DefaultSyscalls {
    fn park(&self) -> io::Result<()> {
        let mut lock = self.inner.lock().unwrap();
        match lock.io_driver.park() {
            Ok(_) => Ok(()),
            Err(Either::A(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("park error: {:?}", e),
            )),
            Err(Either::B(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("park error: {:?}", e),
            )),
        }
    }

    fn park_timeout(&self, duration: std::time::Duration) -> io::Result<()> {
        let mut lock = self.inner.lock().unwrap();
        match lock.io_driver.park_timeout(duration) {
            Ok(_) => Ok(()),
            Err(Either::A(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("park error: {:?}", e),
            )),
            Err(Either::B(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("park error: {:?}", e),
            )),
        }
    }

    fn unpark(&self) {
        let lock = self.inner.lock().unwrap();
        lock.io_driver.unpark(); // TODO: What should be returned here?
    }
}
