use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::{ffi::CString, io, path::Path};

pub(crate) type ArcFd = Arc<dyn AsRawFd + Send + Sync + 'static>;

pub(crate) fn cstr(p: &Path) -> io::Result<CString> {
    Ok(CString::new(p.as_os_str().as_bytes())?)
}
