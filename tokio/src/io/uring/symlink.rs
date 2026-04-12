use super::utils::cstr;

use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::io::Error;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Symlink {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    original: CString,
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    link: CString,
}

impl Completable for Symlink {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        Err(err)
    }
}

impl Cancellable for Symlink {
    fn cancel(self) -> CancelData {
        CancelData::Symlink(self)
    }
}

impl Op<Symlink> {
    /// Submit a request to create a symbolic link.
    pub(crate) fn symlink(original: &Path, link: &Path) -> io::Result<Self> {
        let original = cstr(original)?;
        let link = cstr(link)?;

        let symlink_op =
            opcode::SymlinkAt::new(types::Fd(libc::AT_FDCWD), original.as_ptr(), link.as_ptr())
                .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(symlink_op, Symlink { original, link }) })
    }
}
