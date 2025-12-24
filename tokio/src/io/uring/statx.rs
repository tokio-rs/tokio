use crate::io::uring::utils::{box_new_uninit, cstr};
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use linux_raw_sys::general::statx;
use std::fmt::{Debug, Formatter};
use std::io;
use std::mem::MaybeUninit;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Statx {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    path: std::ffi::CString,
    buffer: Box<MaybeUninit<statx>>,
}

pub(crate) struct Metadata(#[allow(dead_code)] statx);

impl Debug for Metadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metadata").finish_non_exhaustive()
    }
}

impl Completable for Statx {
    type Output = io::Result<Metadata>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        use crate::io::uring::utils::box_assume_init;

        cqe.result
            .map(|_| Metadata(*unsafe { box_assume_init(self.buffer) }))
    }

    fn complete_with_error(self, error: io::Error) -> Self::Output {
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
        let mut buffer = box_new_uninit::<statx>();

        let flags: u32 = linux_raw_sys::general::AT_STATX_SYNC_AS_STAT
            | (linux_raw_sys::general::AT_SYMLINK_NOFOLLOW * u32::from(!follow_symlinks));

        let open_op = opcode::Statx::new(
            types::Fd(libc::AT_FDCWD),
            path.as_ptr(),
            buffer.as_mut_ptr().cast(),
        )
        .flags(flags as i32)
        .mask(linux_raw_sys::general::STATX_BASIC_STATS | linux_raw_sys::general::STATX_BTIME)
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
