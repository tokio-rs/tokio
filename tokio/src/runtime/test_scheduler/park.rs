use crate::park::{Park, Unpark};
use crate::syscall::Syscalls;
use std::io;
use std::sync::Arc;
use std::time::Duration;
pub(crate) struct SyscallsPark {
    syscalls: Arc<dyn Syscalls>,
}

impl SyscallsPark {
    pub(crate) fn new(syscalls: Arc<dyn Syscalls>) -> Self {
        Self { syscalls }
    }
}

impl Park for SyscallsPark {
    type Unpark = SyscallsUnpark;
    type Error = io::Error;

    fn unpark(&self) -> Self::Unpark {
        let syscalls = Arc::clone(&self.syscalls);
        SyscallsUnpark { syscalls }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.syscalls.park()
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.syscalls.park_timeout(duration)
    }
}

pub(crate) struct SyscallsUnpark {
    syscalls: Arc<dyn Syscalls>,
}

impl Unpark for SyscallsUnpark {
    fn unpark(&self) {
        self.syscalls.unpark()
    }
}
