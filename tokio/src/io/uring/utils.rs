use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::{ffi::CString, io, path::Path};

pub(crate) fn cstr(p: &Path) -> io::Result<CString> {
    Ok(CString::new(p.as_os_str().as_bytes())?)
}

/// A reference-counted handle to the `OwnedFd`.
#[derive(Clone, Debug)]
pub(crate) struct SharedFd {
    inner: Arc<OwnedFd>,
}

impl SharedFd {
    pub(crate) fn new(fd: OwnedFd) -> SharedFd {
        SharedFd {
            inner: Arc::new(fd),
        }
    }
}

impl AsRawFd for SharedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl TryFrom<crate::fs::File> for SharedFd {
    type Error = crate::fs::File;
    fn try_from(value: crate::fs::File) -> Result<Self, Self::Error> {
        Ok(SharedFd::new(OwnedFd::from(value.try_into_std()?)))
    }
}
