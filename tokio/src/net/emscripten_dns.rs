//! Name resolution on `wasm32-unknown-emscripten`.
//!
//! The synchronous `getaddrinfo` has nothing to block on there (under
//! `-sNODERAWSOCKETS` a name lookup fails with `EAI_AGAIN`), so the
//! blocking-pool resolver the other targets use never resolves a name.
//! Emscripten's asynchronous `getaddrinfo` instead returns an fd that becomes
//! readable once the lookup completes, which the I/O driver awaits like any
//! other.

use crate::io::unix::AsyncFd;
use crate::io::Interest;

use std::ffi::{c_char, c_int, CStr, CString};
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr;
use std::vec;

extern "C" {
    fn emscripten_dns_lookup_async(
        name: *const c_char,
        service: *const c_char,
        hints: *const libc::addrinfo,
    ) -> c_int;
    fn emscripten_dns_lookup_result(fd: c_int, res: *mut *mut libc::addrinfo) -> c_int;
}

/// Resolves `host` to its addresses, each carrying `port`.
pub(crate) async fn resolve(host: String, port: u16) -> io::Result<vec::IntoIter<SocketAddr>> {
    let name = CString::new(host)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "host contains a NUL byte"))?;
    // SAFETY: `name` outlives the call; null service and hints request the
    // defaults.
    let fd = unsafe { emscripten_dns_lookup_async(name.as_ptr(), ptr::null(), ptr::null()) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a descriptor the lookup just opened for us to close.
    let fd = AsyncFd::with_interest(unsafe { OwnedFd::from_raw_fd(fd) }, Interest::READABLE)?;
    loop {
        let mut guard = fd.readable().await?;
        let mut res: *mut libc::addrinfo = ptr::null_mut();
        // SAFETY: `fd` is the lookup's descriptor; `res` receives the list.
        match unsafe { emscripten_dns_lookup_result(guard.get_inner().as_raw_fd(), &mut res) } {
            0 => {
                // SAFETY: a list the lookup allocated, walked once and freed.
                let addrs = unsafe { collect(res, port) };
                // SAFETY: the same list, no longer referenced.
                unsafe { libc::freeaddrinfo(res) };
                return Ok(addrs.into_iter());
            }
            libc::EAI_AGAIN => guard.clear_ready(),
            code => {
                // SAFETY: `gai_strerror` returns a static string.
                let msg = unsafe { CStr::from_ptr(libc::gai_strerror(code)) };
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    msg.to_string_lossy().into_owned(),
                ));
            }
        }
    }
}

/// The addresses of an `addrinfo` list, each with `port`.
///
/// # Safety
///
/// `ai` is null or the head of a list `getaddrinfo` produced.
unsafe fn collect(mut ai: *const libc::addrinfo, port: u16) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    while !ai.is_null() {
        // SAFETY: a non-null node of the list, whose `ai_addr` matches `ai_family`.
        let a = unsafe { &*ai };
        match a.ai_family {
            libc::AF_INET => {
                let sin = unsafe { &*(a.ai_addr as *const libc::sockaddr_in) };
                let ip = Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
                addrs.push(SocketAddr::new(ip.into(), port));
            }
            libc::AF_INET6 => {
                let sin6 = unsafe { &*(a.ai_addr as *const libc::sockaddr_in6) };
                let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                addrs.push(SocketAddr::new(ip.into(), port));
            }
            _ => {}
        }
        ai = a.ai_next;
    }
    addrs
}
