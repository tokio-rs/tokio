use super::utils::cstr;

use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Link {
    /// These fields will be read by the kernel during the operation, so we
    /// need to ensure they are valid for the entire duration of the operation.
    _original: CString,
    _link: CString,
}

impl Completable for Link {
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

impl Cancellable for Link {
    fn cancel(self) -> CancelData {
        CancelData::Link(self)
    }
}

impl Op<Link> {
    /// Submit a request to create a hard link.
    pub(crate) fn link(original: &Path, link: &Path) -> io::Result<Op<Link>> {
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
        Ok(unsafe {
            Op::new(
                link_op,
                Link {
                    _original: original,
                    _link: link,
                },
            )
        })
    }
}
