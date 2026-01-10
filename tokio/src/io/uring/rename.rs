use super::utils::cstr;

use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Rename {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    from: CString,
    #[allow(dead_code)]
    to: CString,
}

impl Completable for Rename {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(drop)
    }

    fn complete_with_error(self, error: io::Error) -> Self::Output {
        Err(error)
    }
}

impl Cancellable for Rename {
    fn cancel(self) -> CancelData {
        CancelData::Rename(self)
    }
}

impl Op<Rename> {
    pub(crate) const CODE: u8 = opcode::RenameAt::CODE;

    pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<Op<Rename>> {
        let from = cstr(from)?;
        let to = cstr(to)?;

        let rename_op = opcode::RenameAt::new(
            types::Fd(libc::AT_FDCWD),
            from.as_ptr(),
            types::Fd(libc::AT_FDCWD),
            to.as_ptr(),
        )
        .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(rename_op, Rename { from, to }) })
    }
}
