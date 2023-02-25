use crate::io::{LocklessSplit, Shutdown};

use super::UnixStream;

unsafe impl LocklessSplit for UnixStream {}

impl Shutdown for UnixStream {
    fn shutdown(&mut self) {
        let _ = self.shutdown_std(std::net::Shutdown::Write);
    }
}
