#![cfg(all(tokio_uring, target_os = "linux", target_env = "gnu"))]

use libc::{c_char, c_int, c_uint, c_void, dlsym, RTLD_NEXT};
use std::cell::Cell;
use std::path::Path;
use std::ptr;
use std::sync::Once;

thread_local! {
    static STATX_VALUE: Cell<Option<(c_int, *const libc::statx)>> = const { Cell::new(None) };
}

pub fn metadata_with_statx(
    path: &Path,
    rcode: i32,
    statx: *const libc::statx,
) -> std::io::Result<std::fs::Metadata> {
    STATX_VALUE.set(Some((rcode, statx)));
    std::fs::metadata(path)
}

type StatxFn = unsafe extern "C" fn(c_int, *const c_char, c_int, c_uint, *mut libc::statx) -> c_int;

fn real_statx() -> StatxFn {
    static mut REAL: *mut c_void = ptr::null_mut();
    static INIT: Once = Once::new();

    #[allow(clippy::manual_c_str_literals)]
    INIT.call_once(|| unsafe {
        REAL = dlsym(RTLD_NEXT, b"statx\0".as_ptr() as *const _);
        if REAL.is_null() {
            panic!("could not find statx symbol via dlsym");
        }
    });

    unsafe { std::mem::transmute::<*mut c_void, StatxFn>(REAL) }
}

// ugly hack to get real std::fs::Metadata type
// we override statx() symbol
// so that std::fs::metadata will call our version instead of performing actual syscall
#[no_mangle]
pub(crate) unsafe extern "C" fn statx(
    fd: c_int,
    pathname: *const c_char,
    flags: c_int,
    mask: c_uint,
    statxbuf: *mut libc::statx,
) -> c_int {
    if let Some((result, buffer)) = STATX_VALUE.take() {
        *statxbuf = *buffer;
        return result;
    }

    real_statx()(fd, pathname, flags, mask, statxbuf)
}
