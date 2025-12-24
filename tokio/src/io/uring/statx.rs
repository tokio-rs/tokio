use crate::io::uring::utils::{box_assume_init, box_new_uninit, cstr};
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::io::Error;
use std::mem::MaybeUninit;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Statx {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    path: CString,
    buffer: Box<MaybeUninit<libc::statx>>,
}

impl Completable for Statx {
    type Output = io::Result<libc::statx>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| *unsafe { box_assume_init(self.buffer) })
    }

    fn complete_with_error(self, error: Error) -> Self::Output {
        Err(error)
    }
}

impl Cancellable for Statx {
    fn cancel(self) -> CancelData {
        CancelData::Statx(self)
    }
}

impl Op<Statx> {
    /// Submit a request to open a file.
    fn statx(path: &Path, follow_symlinks: bool) -> io::Result<Op<Statx>> {
        let path = cstr(path)?;
        let mut buffer = box_new_uninit::<libc::statx>();

        let flags = libc::AT_STATX_SYNC_AS_STAT
            | (libc::AT_SYMLINK_NOFOLLOW * libc::c_int::from(!follow_symlinks));

        let open_op = opcode::Statx::new(
            types::Fd(libc::AT_FDCWD),
            path.as_ptr(),
            buffer.as_mut_ptr().cast(),
        )
        .flags(flags)
        .mask(libc::STATX_BASIC_STATS | libc::STATX_BTIME)
        .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(open_op, Statx { path, buffer }) })
    }

    pub(crate) fn metadata(path: &Path) -> io::Result<Op<Statx>> {
        Op::statx(path, true)
    }

    // pub(crate) fn symlink_metadata(path: &Path) -> io::Result<Op<Statx>> {
    //     Op::statx(path, false)
    // }
}
