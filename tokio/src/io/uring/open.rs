use super::utils::cstr;

use crate::fs::UringOpenOptions;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::ffi::CString;
use std::io::{self, Error};
use std::os::fd::FromRawFd;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Open {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    path: CString,
}

/// Distinguishes whether an `Interrupted` error allows retrying the open operation.
#[derive(Debug)]
pub(crate) enum OpenError {
    /// A submission error: return it without retrying, even for `Interrupted`.
    Submission(io::Error),
    /// An error reported by the CQE: retry only if it is `Interrupted`.
    Completion(io::Error),
}

impl Completable for Open {
    type Output = Result<crate::fs::File, OpenError>;
    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result
            .map(|fd| unsafe { crate::fs::File::from_raw_fd(fd as i32) })
            .map_err(OpenError::Completion)
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        Err(OpenError::Submission(err))
    }
}

impl Cancellable for Open {
    fn cancel(self) -> CancelData {
        CancelData::Open(self)
    }
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path, options: &UringOpenOptions) -> io::Result<Op<Open>> {
        let inner_opt = options;
        let path = cstr(path)?;

        let custom_flags = inner_opt.custom_flags;
        let flags = libc::O_CLOEXEC
            | options.access_mode()?
            | options.creation_mode()?
            | (custom_flags & !libc::O_ACCMODE);

        let open_op = opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), path.as_ptr())
            .flags(flags)
            .mode(inner_opt.mode)
            .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        let op = unsafe { Op::new(open_op, Open { path }) };
        Ok(op)
    }
}

pub(crate) async fn open(path: &Path, options: &UringOpenOptions) -> io::Result<crate::fs::File> {
    loop {
        match Op::open(path, options)?.await {
            Err(OpenError::Completion(e)) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(OpenError::Completion(e) | OpenError::Submission(e)) => return Err(e),
            Ok(file) => return Ok(file),
        }
    }
}
