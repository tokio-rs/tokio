use crate::park::{Park, Unpark};
use crate::syscall::Syscalls;
use std::sync::Arc;
use std::time::Duration;
pub(super) struct SyscallsPark<P>
where
    P: Park,
{
    park: P,
    syscalls: Arc<dyn Syscalls>,
}

impl<P> SyscallsPark<P>
where
    P: Park,
{
    pub(super) fn new(park: P, syscalls: Arc<dyn Syscalls>) -> Self {
        Self { park, syscalls }
    }
}

impl<P> Park for SyscallsPark<P>
where
    P: Park,
{
    type Unpark = SyscallsUnpark<P::Unpark>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        let syscalls = Arc::clone(&self.syscalls);
        let unpark = self.park.unpark();
        SyscallsUnpark { unpark, syscalls }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.syscalls.park();
        self.park.park()
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.syscalls.park_timeout(duration);
        self.park.park_timeout(duration)
    }
}

pub(super) struct SyscallsUnpark<P>
where
    P: Unpark,
{
    unpark: P,
    syscalls: Arc<dyn Syscalls>,
}

impl<P> Unpark for SyscallsUnpark<P>
where
    P: Unpark,
{
    fn unpark(&self) {
        self.syscalls.unpark();
        self.unpark.unpark();
    }
}
