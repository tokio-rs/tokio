use std::os::unix::ffi::OsStrExt;
use std::{ffi::CString, io, path::Path};

pub(crate) fn cstr(p: &Path) -> io::Result<CString> {
    Ok(CString::new(p.as_os_str().as_bytes())?)
}
