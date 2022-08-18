//! Stub implementations for the platform API so that rustdoc can build linkable
//! documentation on non-windows platforms.

use crate::signal::RxFuture;
use std::io;

pub(super) fn ctrl_break() -> io::Result<RxFuture> {
    panic!()
}

pub(super) fn crtl_close() -> io::Result<RxFuture> {
    panic!()
}

pub(super) fn ctrl_c() -> io::Result<RxFuture> {
    panic!()
}

pub(super) fn crtl_log_off() -> io::Result<RxFuture> {
    panic!()
}

pub(super) fn crtl_shutdown() -> io::Result<RxFuture> {
    panic!()
}
