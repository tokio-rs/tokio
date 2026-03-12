use super::utils::cstr;

use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::io::Error;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Link {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    original: CString,
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    link: CString,
}

impl Completable for Link {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        Err(err)
    }
}

impl Cancellable for Link {
    fn cancel(self) -> CancelData {
        CancelData::Link(self)
    }
}

impl Op<Link> {
    /// Submit a request to create a hard link.
    pub(crate) fn link(original: &Path, link: &Path) -> io::Result<Self> {
        let original = cstr(original)?;
        let link = cstr(link)?;

        let link_op = opcode::LinkAt::new(
            types::Fd(libc::AT_FDCWD),
            original.as_ptr(),
            types::Fd(libc::AT_FDCWD),
            link.as_ptr(),
        )
        .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(link_op, Link { original, link }) })
    }
}
