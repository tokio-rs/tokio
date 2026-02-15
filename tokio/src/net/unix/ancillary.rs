use std::fmt;
use std::marker::PhantomData;
use std::mem;
use std::os::unix::io::RawFd;

/// Error returned when an ancillary data operation fails.
///
/// This typically occurs when the buffer provided to [`SocketAncillary`]
/// is too small to hold the requested control messages.
#[derive(Debug)]
pub struct AncillaryError;

impl fmt::Display for AncillaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ancillary buffer too small")
    }
}

impl std::error::Error for AncillaryError {}

/// Ancillary data received from a Unix socket control message.
///
/// This enum is yielded by the [`Messages`] iterator when parsing
/// received ancillary data from [`SocketAncillary`].
pub enum AncillaryData<'a> {
    /// A set of file descriptors received via `SCM_RIGHTS`.
    ///
    /// File descriptors received this way are new descriptors in the
    /// receiving process. The caller is responsible for closing them
    /// (e.g., by wrapping in [`OwnedFd`](std::os::unix::io::OwnedFd)).
    ScmRights(ScmRights<'a>),
}

impl fmt::Debug for AncillaryData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AncillaryData::ScmRights(_) => f.debug_tuple("ScmRights").finish(),
        }
    }
}

/// An iterator over file descriptors received via `SCM_RIGHTS`.
///
/// This struct is created by [`AncillaryData::ScmRights`].
/// Each call to [`next()`](Iterator::next) yields a [`RawFd`].
/// The caller is responsible for managing the lifetime of these
/// file descriptors.
pub struct ScmRights<'a> {
    data: &'a [u8],
    offset: usize,
}

impl fmt::Debug for ScmRights<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScmRights").finish()
    }
}

impl<'a> ScmRights<'a> {
    fn new(data: &'a [u8]) -> Self {
        ScmRights { data, offset: 0 }
    }
}

impl Iterator for ScmRights<'_> {
    type Item = RawFd;

    fn next(&mut self) -> Option<Self::Item> {
        let fd_size = mem::size_of::<RawFd>();
        if self.offset + fd_size > self.data.len() {
            return None;
        }
        let mut fd_bytes = [0u8; mem::size_of::<RawFd>()];
        fd_bytes.copy_from_slice(&self.data[self.offset..self.offset + fd_size]);
        self.offset += fd_size;
        Some(RawFd::from_ne_bytes(fd_bytes))
    }
}

/// An iterator over control messages in a [`SocketAncillary`] buffer.
///
/// Created by [`SocketAncillary::messages`].
pub struct Messages<'a> {
    current: *const libc::cmsghdr,
    msg: libc::msghdr,
    _marker: PhantomData<&'a [u8]>,
}

impl fmt::Debug for Messages<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Messages").finish()
    }
}

// SAFETY: Messages only reads from a shared reference to the buffer.
unsafe impl Send for Messages<'_> {}
// SAFETY: Messages only reads from a shared reference to the buffer.
unsafe impl Sync for Messages<'_> {}

impl<'a> Messages<'a> {
    fn new(buffer: &'a [u8], length: usize) -> Self {
        let mut msg: libc::msghdr = unsafe { mem::zeroed() };
        msg.msg_control = buffer.as_ptr() as *mut libc::c_void;
        msg.msg_controllen = length as _;

        let current = unsafe { libc::CMSG_FIRSTHDR(&msg) };

        Messages {
            current,
            msg,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for Messages<'a> {
    type Item = AncillaryData<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }

        unsafe {
            let cmsg = &*self.current;

            // Advance to next header for the next call
            self.current = libc::CMSG_NXTHDR(&self.msg, self.current);

            if cmsg.cmsg_level == libc::SOL_SOCKET && cmsg.cmsg_type == libc::SCM_RIGHTS {
                let data_ptr = libc::CMSG_DATA(cmsg as *const _ as *mut _);
                #[allow(clippy::unnecessary_cast)]
                let data_len =
                    cmsg.cmsg_len as usize - (data_ptr as usize - cmsg as *const _ as usize);
                let data = std::slice::from_raw_parts(data_ptr, data_len);
                Some(AncillaryData::ScmRights(ScmRights::new(data)))
            } else {
                // Skip unknown control message types, try next
                self.next()
            }
        }
    }
}

/// A buffer for sending and receiving ancillary data over Unix sockets.
///
/// This type wraps a user-provided byte buffer and provides methods to
/// construct outgoing control messages (for sending file descriptors)
/// and parse incoming control messages (for receiving file descriptors).
///
/// # Usage
///
/// ## Sending file descriptors
///
/// ```no_run
/// use tokio::net::unix::SocketAncillary;
/// use std::os::unix::io::AsRawFd;
///
/// let file = std::fs::File::open("/dev/null").unwrap();
/// let mut buf = [0u8; 128];
/// let mut ancillary = SocketAncillary::new(&mut buf);
/// ancillary.add_fds(&[file.as_raw_fd()]).unwrap();
/// ```
///
/// ## Receiving file descriptors
///
/// ```no_run
/// use tokio::net::unix::{SocketAncillary, AncillaryData};
///
/// let mut buf = [0u8; 128];
/// let ancillary = SocketAncillary::new(&mut buf);
/// // After a recv_vectored_with_ancillary call:
/// for msg in ancillary.messages() {
///     match msg {
///         AncillaryData::ScmRights(fds) => {
///             for fd in fds {
///                 println!("received fd: {}", fd);
///             }
///         }
///     }
/// }
/// ```
pub struct SocketAncillary<'a> {
    buffer: &'a mut [u8],
    length: usize,
    truncated: bool,
}

impl fmt::Debug for SocketAncillary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketAncillary")
            .field("length", &self.length)
            .field("truncated", &self.truncated)
            .finish()
    }
}

impl<'a> SocketAncillary<'a> {
    /// Creates a new `SocketAncillary` wrapping the provided buffer.
    ///
    /// The buffer is used to hold raw control message data. Use
    /// [`buffer_size_for_rights`](Self::buffer_size_for_rights) to
    /// determine the minimum buffer size needed.
    pub fn new(buffer: &'a mut [u8]) -> Self {
        SocketAncillary {
            buffer,
            length: 0,
            truncated: false,
        }
    }

    /// Returns the minimum buffer size needed to send or receive
    /// `num_fds` file descriptors via `SCM_RIGHTS`.
    pub fn buffer_size_for_rights(num_fds: usize) -> usize {
        unsafe { libc::CMSG_SPACE((num_fds * mem::size_of::<RawFd>()) as libc::c_uint) as usize }
    }

    /// Adds file descriptors to be sent as `SCM_RIGHTS`.
    ///
    /// Returns `Err(AncillaryError)` if the buffer is too small.
    pub fn add_fds(&mut self, fds: &[RawFd]) -> Result<(), AncillaryError> {
        let fd_bytes_len = std::mem::size_of_val(fds);
        let space = unsafe { libc::CMSG_SPACE(fd_bytes_len as libc::c_uint) as usize };

        if self.length + space > self.buffer.len() {
            return Err(AncillaryError);
        }

        unsafe {
            // Build a temporary msghdr pointing at our buffer to use CMSG macros
            let mut msg: libc::msghdr = mem::zeroed();
            msg.msg_control = self.buffer.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = (self.length + space) as _;

            // Find the cmsg slot. If length is 0, use CMSG_FIRSTHDR.
            // Otherwise walk to find the next available slot.
            let cmsg = if self.length == 0 {
                libc::CMSG_FIRSTHDR(&msg)
            } else {
                // Point msg_controllen at current length to walk existing headers
                let mut walk_msg: libc::msghdr = mem::zeroed();
                walk_msg.msg_control = self.buffer.as_mut_ptr() as *mut libc::c_void;
                walk_msg.msg_controllen = self.length as _;

                let mut cur = libc::CMSG_FIRSTHDR(&walk_msg);
                while !cur.is_null() {
                    let next = libc::CMSG_NXTHDR(&walk_msg, cur);
                    if next.is_null() {
                        // cur is the last header; the next slot is after it
                        break;
                    }
                    cur = next;
                }
                // Recalculate with expanded controllen
                msg.msg_controllen = (self.length + space) as _;
                if cur.is_null() {
                    libc::CMSG_FIRSTHDR(&msg)
                } else {
                    libc::CMSG_NXTHDR(&msg, cur)
                }
            };

            if cmsg.is_null() {
                return Err(AncillaryError);
            }

            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(fd_bytes_len as libc::c_uint) as _;

            let data_ptr = libc::CMSG_DATA(cmsg);
            std::ptr::copy_nonoverlapping(fds.as_ptr() as *const u8, data_ptr, fd_bytes_len);
        }

        self.length += space;
        Ok(())
    }

    /// Returns `true` if the ancillary data was truncated during receive.
    ///
    /// This happens when the buffer provided was too small to hold all
    /// control messages. Any file descriptors that did not fit are lost.
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Returns an iterator over the received control messages.
    pub fn messages(&self) -> Messages<'_> {
        Messages::new(&self.buffer[..self.length], self.length)
    }

    /// Clears all ancillary data, resetting the buffer for reuse.
    pub fn clear(&mut self) {
        self.length = 0;
        self.truncated = false;
    }

    /// Returns the number of valid bytes of ancillary data.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Returns `true` if there is no ancillary data.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    // -- pub(crate) helpers for cmsg.rs integration --

    /// Returns a pointer to the raw buffer and its total capacity.
    pub(crate) fn as_buffer(&self) -> &[u8] {
        &self.buffer[..self.length]
    }

    /// Returns a mutable reference to the full buffer for receiving.
    pub(crate) fn as_mut_buffer(&mut self) -> &mut [u8] {
        self.buffer
    }

    /// Called after recvmsg to set the valid ancillary data length and
    /// truncation flag.
    pub(crate) fn set_received(&mut self, length: usize, truncated: bool) {
        self.length = length;
        self.truncated = truncated;
    }

    /// On platforms without `MSG_CMSG_CLOEXEC`, iterate received fds
    /// and set `FD_CLOEXEC` via `fcntl`.
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    pub(crate) fn set_cloexec(&self) -> std::io::Result<()> {
        for msg in self.messages() {
            if let AncillaryData::ScmRights(fds) = msg {
                for fd in fds {
                    super::cmsg::set_cloexec(fd)?;
                }
            }
        }
        Ok(())
    }
}
