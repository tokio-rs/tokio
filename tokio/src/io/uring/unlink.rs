use crate::io::uring::utils::cstr;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::io::Error;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Unlink {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    _path: CString,
}

impl Completable for Unlink {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(drop)
    }

    fn complete_with_error(self, error: Error) -> Self::Output {
        Err(error)
    }
}

impl Cancellable for Unlink {
    fn cancel(self) -> CancelData {
        CancelData::Unlink(self)
    }
}

impl Op<Unlink> {
    pub(crate) const CODE: u8 = opcode::UnlinkAt::CODE;

    /// Submit a request to unlink a file or directory.
    fn unlink(path: &Path, flags: libc::c_int) -> io::Result<Op<Unlink>> {
        let path = cstr(path)?;

        let unlink_op = opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path.as_ptr())
            .flags(flags)
            .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(unlink_op, Unlink { _path: path }) })
    }

    pub(crate) fn remove_file(path: &Path) -> io::Result<Op<Unlink>> {
        Self::unlink(path, 0)
    }

    pub(crate) fn remove_dir(path: &Path) -> io::Result<Op<Unlink>> {
        Self::unlink(path, libc::AT_REMOVEDIR)
    }
}
