use std::os::unix::io::{AsRawFd, RawFd};

use crate::File;

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.std.as_raw_fd()
    }
}
