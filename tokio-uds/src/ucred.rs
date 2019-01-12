use libc::{gid_t, uid_t};

/// Credentials of a process
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct UCred {
    /// UID (user ID) of the process
    pub uid: uid_t,
    /// GID (group ID) of the process
    pub gid: gid_t,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use self::impl_linux::get_peer_cred;

#[cfg(any(target_os = "dragonfly", target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
pub use self::impl_macos::get_peer_cred;

#[cfg(any(target_os = "solaris"))]
pub use self::impl_solaris::get_peer_cred;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod impl_linux {
    use libc::{c_void, getsockopt, socklen_t, SOL_SOCKET, SO_PEERCRED};
    use std::{io, mem};
    use UnixStream;
    use std::os::unix::io::AsRawFd;

    use libc::ucred;

    pub fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut ucred = ucred {
                pid: 0,
                uid: 0,
                gid: 0,
            };

            let ucred_size = mem::size_of::<ucred>();

            // These paranoid checks should be optimized-out
            assert!(mem::size_of::<u32>() <= mem::size_of::<usize>());
            assert!(ucred_size <= u32::max_value() as usize);

            let mut ucred_size = ucred_size as socklen_t;

            let ret = getsockopt(
                raw_fd,
                SOL_SOCKET,
                SO_PEERCRED,
                &mut ucred as *mut ucred as *mut c_void,
                &mut ucred_size,
            );
            if ret == 0 && ucred_size as usize == mem::size_of::<ucred>() {
                Ok(super::UCred {
                    uid: ucred.uid,
                    gid: ucred.gid,
                })
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}

#[cfg(any(target_os = "dragonfly", target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
pub mod impl_macos {
    use libc::getpeereid;
    use std::{io, mem};
    use UnixStream;
    use std::os::unix::io::AsRawFd;

    pub fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut cred: super::UCred = mem::uninitialized();

            let ret = getpeereid(raw_fd, &mut cred.uid, &mut cred.gid);

            if ret == 0 {
                Ok(cred)
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}


#[cfg(any(target_os = "solaris"))]
pub mod impl_solaris {
    use std::io;
    use std::os::unix::io::AsRawFd;
    use UnixStream;
    use std::ptr;

    #[allow(non_camel_case_types)]
    enum ucred_t {}

    extern "C" {
        fn ucred_free(cred: *mut ucred_t);
        fn ucred_geteuid(cred: *const ucred_t) -> super::uid_t;
        fn ucred_getegid(cred: *const ucred_t) -> super::gid_t;

        fn getpeerucred(fd: ::std::os::raw::c_int, cred: *mut *mut ucred_t) -> ::std::os::raw::c_int;
    }

    pub fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut cred = ptr::null_mut::<*mut ucred_t>() as *mut ucred_t;

            let ret = getpeerucred(raw_fd, &mut cred);

            if ret == 0 {
                let uid = ucred_geteuid(cred);
                let gid = ucred_getegid(cred);

                ucred_free(cred);

                Ok(super::UCred {
                    uid,
                    gid,
                })
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}


// Note that LOCAL_PEERCRED is not supported on DragonFly (yet). So do not run tests.
#[cfg(not(target_os = "dragonfly"))]
#[cfg(test)]
mod test {
    use UnixStream;
    use libc::geteuid;
    use libc::getegid;

    #[test]
    #[cfg_attr(target_os = "freebsd", ignore = "Requires FreeBSD 12.0 or later. https://bugs.freebsd.org/bugzilla/show_bug.cgi?id=176419")]
    #[cfg_attr(target_os = "netbsd", ignore = "NetBSD does not support getpeereid() for sockets created by socketpair()")]
    fn test_socket_pair() {
        let (a, b) = UnixStream::pair().unwrap();
        let cred_a = a.peer_cred().unwrap();
        let cred_b = b.peer_cred().unwrap();
        assert_eq!(cred_a, cred_b);

        let uid = unsafe { geteuid() };
        let gid = unsafe { getegid() };

        assert_eq!(cred_a.uid, uid);
        assert_eq!(cred_a.gid, gid);
    }
}
