use crate::io::{LocklessSplit, Shutdown};

use super::TcpStream;

unsafe impl LocklessSplit for TcpStream {}

impl Shutdown for TcpStream {
    fn shutdown(&mut self) {
        let _ = self.shutdown_std(std::net::Shutdown::Write);
    }
}
