use crate::platform::linux::uring::driver::{self, Op};
use crate::platform::linux::uring::fs::OpenOptions;

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Open a file
#[allow(dead_code)]
pub(crate) struct Open {
    pub(crate) path: CString,
    pub(crate) flags: libc::c_int,
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path, options: &OpenOptions) -> io::Result<Op<Open>> {
        use io_uring::{opcode, types};
        let path = driver::util::cstr(path)?;
        let flags = libc::O_CLOEXEC | options.access_mode()? | options.creation_mode()?;

        Op::submit_with(Open { path, flags }, |open| {
            // Get a reference to the memory. The string will be held by the
            // operation state and will not be accessed again until the operation
            // completes.
            let p_ref = open.path.as_c_str().as_ptr();

            opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                .flags(flags)
                .mode(options.mode)
                .build()
        })
    }
}
