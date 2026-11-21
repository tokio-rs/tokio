use crate::io::uring::utils::{ArcFd, UringFd};
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::fmt;
use std::io;

pub(crate) struct Fsync {
    /// The kernel uses this descriptor during the operation, so it must stay
    /// open until the operation completes.
    _fd: ArcFd,
}

impl fmt::Debug for Fsync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fsync").finish_non_exhaustive()
    }
}

impl Completable for Fsync {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(drop)
    }

    fn complete_with_error(self, error: io::Error) -> Self::Output {
        Err(error)
    }

    fn complete_after_cancel(error: io::Error) -> Self::Output {
        Err(error)
    }
}

impl Cancellable for Fsync {
    fn cancel(self) -> CancelData {
        CancelData::Fsync(self)
    }
}

impl Op<Fsync> {
    /// Flushes the data and metadata of `fd` to disk, like
    /// [`std::fs::File::sync_all`].
    pub(crate) fn sync_all(fd: ArcFd) -> Self {
        Op::fsync(fd, types::FsyncFlags::empty())
    }

    /// Flushes the data of `fd` to disk, and only the metadata needed to read
    /// it back, like [`std::fs::File::sync_data`].
    pub(crate) fn sync_data(fd: ArcFd) -> Self {
        Op::fsync(fd, types::FsyncFlags::DATASYNC)
    }

    fn fsync(fd: ArcFd, flags: types::FsyncFlags) -> Self {
        let sqe = opcode::Fsync::new(types::Fd(UringFd::as_raw_fd(&fd)))
            .flags(flags)
            .build();

        // SAFETY: `fd` is moved into `Fsync`, which the `Op` holds for the
        // entire duration of the operation, so the descriptor stays open.
        unsafe { Op::new(sqe, Fsync { _fd: fd }) }
    }
}
