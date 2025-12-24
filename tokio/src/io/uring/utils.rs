use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::{ffi::CString, io, path::Path};

pub(crate) type ArcFd = Arc<dyn AsRawFd + Send + Sync + 'static>;

/// Raw file descriptor trait for io-uring operations.
///
/// `Arc<dyn AsRawFd>` does not satisfy `AsRawFd` because the blanket impl
/// for `Arc<T>` requires `T: Sized`. This trait bridges that gap so both
/// `OwnedFd` and `ArcFd` can be used generically with `Op::read_at`.
pub(crate) trait UringFd: Send + Sync + 'static {
    fn as_raw_fd(&self) -> RawFd;
}

impl UringFd for OwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        AsRawFd::as_raw_fd(self)
    }
}

impl UringFd for ArcFd {
    fn as_raw_fd(&self) -> RawFd {
        (**self).as_raw_fd()
    }
}

pub(crate) fn cstr(p: &Path) -> io::Result<CString> {
    Ok(CString::new(p.as_os_str().as_bytes())?)
}

// TODO: Remove this once we bump the MSRV to 1.82.
pub(crate) fn box_new_uninit<T>() -> Box<MaybeUninit<T>> {
    // Box::<T>::new_uninit()
    Box::new(MaybeUninit::uninit())
}

// TODO: Remove this once we bump the MSRV to 1.82.
pub(crate) unsafe fn box_assume_init<T>(boxed: Box<MaybeUninit<T>>) -> Box<T> {
    // Box::<MaybeUninit<T>>::assume_init()
    let raw = Box::into_raw(boxed);
    unsafe { Box::from_raw(raw as *mut T) }
}
