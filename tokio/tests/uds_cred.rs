#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(all(unix, not(target_os = "dragonfly"), not(miri)))] // No `getsockopt` on miri.

use tokio::net::UnixStream;

use libc::getegid;
use libc::geteuid;

#[tokio::test]
#[cfg_attr(
    target_os = "netbsd",
    ignore = "NetBSD does not support getpeereid() for sockets created by socketpair()"
)]
async fn test_socket_pair() {
    let (a, b) = UnixStream::pair().unwrap();
    let cred_a = a.peer_cred().unwrap();
    let cred_b = b.peer_cred().unwrap();
    assert_eq!(cred_a, cred_b);

    let uid = unsafe { geteuid() };
    let gid = unsafe { getegid() };

    assert_eq!(cred_a.uid(), uid);
    assert_eq!(cred_a.gid(), gid);

    // On platforms where `UCred::pid` is implemented and the kernel
    // populates it, both ends of a `socketpair` must report the current
    // process's PID.
    //
    // FreeBSD COMPAT32 (32-bit binary on a 64-bit kernel) leaves `cr_pid`
    // zeroed pending FreeBSD bug 294833; the assertion is gated on 64-bit
    // FreeBSD until that fix ships.
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "openbsd",
        all(target_os = "freebsd", target_pointer_width = "64"),
        target_os = "netbsd",
        target_os = "nto",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "visionos",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "redox",
        target_os = "haiku",
        target_os = "cygwin",
    ))]
    {
        let pid = unsafe { libc::getpid() };
        assert_eq!(cred_a.pid(), Some(pid));
        assert_eq!(cred_b.pid(), Some(pid));
    }
}
