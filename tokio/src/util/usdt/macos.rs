//! USDT for macOS.
//!
//! Probe discovery is based on link names
//!
//! To generate the following extern "C" block, run the following:
//!
//! ```sh
//! dtrace -h -s provider.d -o /dev/stdout
//! ```
//!
//! How to construct the probe names manually:
//! ```rust,ignore
//! let provider = "tokio";
//! // dtrace convention: `__` implies a `-`,
//! let probe = "task__details"
//! // the probe arguments as dtrace types.
//! let args = ["uint64_t", "char *", "char *", "uint32_t", "uint32_t"];
//!
//! let mut link_name = format!("__dtrace_probe${provider}${probe}$v1");
//! for arg in args {
//!     link_name.push('$');
//!     link_name.push_str(hex::encode(arg));
//! }
//! ```
//!
//! A list of common types:
//! * `*const core::ffi::c_char` -> `char *` -> `63686172202a`
//! * `usize` -> `uintptr_t` -> `75696e747074725f74`
//! * `u64` -> `uint64_t` -> `75696e7436345f74`
//! * `u32` -> `uint32_t` -> `75696e7433325f74`
//! * `u8` -> `uint8_t` -> `75696e74385f74`
//!
//! Since these are regular functions, the arguments must be passed in

use std::arch::global_asm;

#[inline(always)]
pub(super) fn task_details(task_id: u64, name: &str, file: &str, line: u32, col: u32) {
    unsafe extern "C" {
        #[link_name = "__dtrace_isenabled$tokio$task__details$v1"]
        fn task_details_enabled() -> i32;

        #[link_name = "__dtrace_probe$tokio$task__details$v1$75696e7436345f74$63686172202a$63686172202a$75696e7433325f74$75696e7433325f74"]
        #[cold]
        fn __task_details(
            task_id: u64,
            name: *const core::ffi::c_char,
            file: *const core::ffi::c_char,
            line: u32,
            col: u32,
        );
    }

    if unsafe { task_details_enabled() } != 0 {
        // add nul bytes
        let name0 = [name.as_bytes(), b"\0"].concat();
        let file0 = [file.as_bytes(), b"\0"].concat();

        unsafe {
            __task_details(
                task_id,
                name0.as_ptr() as *const std::ffi::c_char,
                file0.as_ptr() as *const std::ffi::c_char,
                line,
                col,
            );
        }
    }
}

#[inline(always)]
pub(super) fn task_start(task_id: u64, spawned: u8, size: usize, original_size: usize) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$task__start$v1$75696e7436345f74$75696e74385f74$75696e747074725f74$75696e747074725f74"]
        fn __task_start(task_id: u64, kind: u8, size: usize, original_size: usize);
    }

    unsafe {
        __task_start(task_id, spawned, size, original_size);
    }
}

#[inline(always)]
pub(super) fn task_poll_start(task_id: u64) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$task__poll__start$v1$75696e7436345f74"]
        fn __task_poll_start(task_id: core::ffi::c_ulonglong);
    }

    unsafe { __task_poll_start(task_id) }
}

#[inline(always)]
pub(super) fn task_poll_end(task_id: u64) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$task__poll__end$v1$75696e7436345f74"]
        fn __task_poll_end(task_id: core::ffi::c_ulonglong);
    }

    unsafe { __task_poll_end(task_id) }
}

#[inline(always)]
pub(crate) fn task_terminate(task_id: u64, reason: u8) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$task__terminate$v1$75696e7436345f74$75696e74385f74"]
        fn __task_terminate(task_id: core::ffi::c_ulonglong, reason: core::ffi::c_uchar);
    }

    unsafe { __task_terminate(task_id, reason) }
}

#[inline(always)]
pub(crate) fn waker_clone(task_id: u64) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$waker__clone$v1$75696e7436345f74"]
        fn __waker_clone(task_id: core::ffi::c_ulonglong);
    }

    unsafe { __waker_clone(task_id) }
}

#[inline(always)]
pub(crate) fn waker_drop(task_id: u64) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$waker__drop$v1$75696e7436345f74"]
        fn __waker_drop(task_id: core::ffi::c_ulonglong);
    }

    unsafe { __waker_drop(task_id) }
}

#[inline(always)]
pub(crate) fn waker_wake(task_id: u64) {
    extern "C" {
        #[link_name = "__dtrace_probe$tokio$waker__wake$v1$75696e7436345f74"]
        fn __waker_wake(task_id: core::ffi::c_ulonglong);
    }

    unsafe { __waker_wake(task_id) }
}

unsafe extern "C" {
    #[link_name = "__dtrace_stability$tokio$v1$1_1_0_1_1_0_1_1_0_1_1_0_1_1_0"]
    fn stability();

    #[link_name = "__dtrace_typedefs$tokio$v2"]
    fn typedefs();
}

global_asm!(
    ".reference {typedefs}
    .reference {stability}",
    typedefs = sym typedefs,
    stability = sym stability,
);
