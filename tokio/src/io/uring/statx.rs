use crate::io::uring::utils::{box_assume_init, box_new_uninit, cstr};
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use linux_raw_sys::general::statx;
use std::fmt::{Debug, Formatter};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;

pub(crate) struct Metadata(statx);

impl Metadata {
    pub(crate) fn len(&self) -> u64 {
        self.0.stx_size
    }
}

impl Debug for Metadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Metadata");
        debug.field("len", &self.len());
        debug.finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct Statx {
    /// This field will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    #[allow(dead_code)]
    path: std::ffi::CString,
    buffer: Box<MaybeUninit<statx>>,
}

impl Completable for Statx {
    type Output = io::Result<Metadata>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
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

        let statx_op = opcode::Statx::new(
            types::Fd(libc::AT_FDCWD),
            path.as_ptr(),
            buffer.as_mut_ptr().cast(),
        )
        .flags(flags as i32)
        .mask(linux_raw_sys::general::STATX_BASIC_STATS)
        .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        Ok(unsafe { Op::new(statx_op, Statx { path, buffer }) })
    }

    pub(crate) fn metadata(path: &Path) -> io::Result<Op<Statx>> {
        Op::statx(path, true)
    }

    // pub(crate) fn symlink_metadata(path: &Path) -> io::Result<Op<Statx>> {
    //     Op::statx(path, false)
    // }
}

#[derive(Debug)]
pub(crate) struct StatxFd {
    fd: OwnedFd,
    buffer: Box<MaybeUninit<statx>>,
}

impl Completable for StatxFd {
    type Output = (io::Result<Metadata>, OwnedFd);

    fn complete(self, cqe: CqeResult) -> Self::Output {
        let ret = cqe
            .result
            .map(|_| Metadata(*unsafe { box_assume_init(self.buffer) }));

        (ret, self.fd)
    }

    fn complete_with_error(self, error: io::Error) -> Self::Output {
        (Err(error), self.fd)
    }
}

impl Cancellable for StatxFd {
    fn cancel(self) -> CancelData {
        CancelData::StatxFd(self)
    }
}

impl Op<StatxFd> {
    pub(crate) fn metadata_fd(fd: OwnedFd) -> Op<StatxFd> {
        let mut buffer = box_new_uninit::<statx>();

        let flags: u32 =
            linux_raw_sys::general::AT_STATX_SYNC_AS_STAT | linux_raw_sys::general::AT_EMPTY_PATH;

        // io-uring was introduced in linux 5.1
        // pass in an empty path instead of null as specified by man
        // https://man7.org/linux/man-pages/man2/statx.2.html
        let statx_op = opcode::Statx::new(
            types::Fd(fd.as_raw_fd()),
            c"".as_ptr(),
            buffer.as_mut_ptr().cast(),
        )
        .flags(flags as i32)
        .mask(linux_raw_sys::general::STATX_BASIC_STATS)
        .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        unsafe { Op::new(statx_op, StatxFd { fd, buffer }) }
    }
}
