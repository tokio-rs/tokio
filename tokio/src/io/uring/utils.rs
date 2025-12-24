use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::{ffi::CString, io, path::Path};

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
