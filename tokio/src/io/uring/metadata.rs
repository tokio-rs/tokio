use super::utils::cstr;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use std::fmt::Formatter;
use std::{
    ffi::{CString, OsStr},
    fmt, io,
    os::unix::ffi::OsStrExt,
    path::Path,
};
use tokio_shim::metadata_with_statx;

#[derive(Debug)]
pub(crate) struct Metadata {
    /// This fields will be read by the kernel during the operation, so we
    /// need to ensure it is valid for the entire duration of the operation.
    path: CString,
    file_attr: HeapStatx,
}

struct HeapStatx(Box<libc::statx>);

impl HeapStatx {
    fn fmt_ts(ts: libc::statx_timestamp) -> String {
        let sec = ts.tv_sec;
        let nsec = ts.tv_nsec;
        format!("{sec}.{nsec:09}")
    }
}

impl fmt::Debug for HeapStatx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s: &libc::statx = &self.0;

        f.debug_struct("HeapStatx")
            .field("stx_mask", &format_args!("0x{:x}", s.stx_mask))
            .field("stx_blksize", &s.stx_blksize)
            .field("stx_attributes", &format_args!("0x{:x}", s.stx_attributes))
            .field("stx_nlink", &s.stx_nlink)
            .field("stx_uid", &s.stx_uid)
            .field("stx_gid", &s.stx_gid)
            .field("stx_mode", &format_args!("0o{:o}", s.stx_mode))
            .field("stx_ino", &s.stx_ino)
            .field("stx_size", &s.stx_size)
            .field("stx_blocks", &s.stx_blocks)
            .field(
                "stx_attributes_mask",
                &format_args!("0x{:x}", s.stx_attributes_mask),
            )
            .field("stx_atime", &HeapStatx::fmt_ts(s.stx_atime))
            .field("stx_btime", &HeapStatx::fmt_ts(s.stx_btime))
            .field("stx_ctime", &HeapStatx::fmt_ts(s.stx_ctime))
            .finish()
    }
}

impl Completable for Metadata {
    type Output = std::fs::Metadata;
    fn complete(self, cqe: CqeResult) -> io::Result<Self::Output> {
        let path = Path::new(OsStr::from_bytes(self.path.as_bytes()));

        metadata_with_statx(path, cqe.result? as i32, self.file_attr.0.as_ref())
    }
}

impl Cancellable for Metadata {
    fn cancel(self) -> CancelData {
        CancelData::Metadata(self)
    }
}

impl Op<Metadata> {
    pub(crate) fn metadata(path: &Path) -> io::Result<Self> {
        let path = cstr(path)?;
        let mut file_attr = HeapStatx(Box::new(unsafe { std::mem::zeroed() }));
        let statx_buffer: *mut libc::statx = file_attr.0.as_mut();

        let sqe = opcode::Statx::new(
            types::Fd(libc::AT_FDCWD),
            path.as_ptr(),
            statx_buffer.cast(),
        )
        .flags(libc::AT_STATX_SYNC_AS_STAT)
        .mask(libc::STATX_BASIC_STATS | libc::STATX_BTIME)
        .build();

        // SAFETY: parameters of the entry, such as `path` and `file_attr`, are valid
        // until this operation completes.
        let op = unsafe { Op::new(sqe, Metadata { path, file_attr }) };
        Ok(op)
    }
}
