use super::utils::cstr;

use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io;
use std::io::Error;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Mkdir {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    path: CString,
}

impl Completable for Mkdir {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        Err(err)
    }
}

impl Cancellable for Mkdir {
    fn cancel(self) -> CancelData {
        CancelData::Mkdir(self)
    }
}

impl Op<Mkdir> {
    /// Submit a request to create a directory.
    pub(crate) fn mkdir(path: &Path) -> io::Result<Self> {
        let path = cstr(path)?;

        let mkdir_op = opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), path.as_ptr())
            .mode(0o777)
            .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(mkdir_op, Mkdir { path }) })
    }
}
