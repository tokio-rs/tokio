use crate::platform::linux::uring::driver::{self, Op};

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Renames a file, moving it between directories if required.
///
/// The given paths are interpreted relative to the current working directory
/// of the calling process.
pub(crate) struct RenameAt {
    pub(crate) from: CString,
    pub(crate) to: CString,
}

impl Op<RenameAt> {
    /// Submit a request to rename a specified path to a new name with
    /// the provided flags.
    pub(crate) fn rename_at(from: &Path, to: &Path, flags: u32) -> io::Result<Op<RenameAt>> {
        use io_uring::{opcode, types};

        let from = driver::util::cstr(from)?;
        let to = driver::util::cstr(to)?;

        Op::submit_with(RenameAt { from, to }, |rename| {
            // Get a reference to the memory. The string will be held by the
            // operation state and will not be accessed again until the operation
            // completes.
            let from_ref = rename.from.as_c_str().as_ptr();
            let to_ref = rename.to.as_c_str().as_ptr();
            opcode::RenameAt::new(
                types::Fd(libc::AT_FDCWD),
                from_ref,
                types::Fd(libc::AT_FDCWD),
                to_ref,
            )
            .flags(flags)
            .build()
        })
    }
}
