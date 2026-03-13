#![allow(unreachable_pub)]
//! Mock version of `std::fs::OpenOptions`;
use mockall::mock;

use crate::fs::mocks::MockFile;
use std::{io, path::Path};

mock! {
    #[derive(Debug)]
    pub OpenOptions {
        pub fn append(&mut self, append: bool) -> &mut Self;
        pub fn create(&mut self, create: bool) -> &mut Self;
        pub fn create_new(&mut self, create_new: bool) -> &mut Self;
        pub fn open<P: AsRef<Path> + 'static>(&self, path: P) -> io::Result<MockFile>;
        pub fn read(&mut self, read: bool) -> &mut Self;
        pub fn truncate(&mut self, truncate: bool) -> &mut Self;
        pub fn write(&mut self, write: bool) -> &mut Self;

        // Not mocking OpenOptionsExt trait due to:
        // https://github.com/rust-lang/rust/issues/153486
        #[cfg(unix)]
        pub fn custom_flags(&mut self, flags: i32) -> &mut Self;
        #[cfg(unix)]
        pub fn mode(&mut self, mode: u32) -> &mut Self;
        #[cfg(windows)]
        pub fn access_mode(&mut self, access: u32) -> &mut Self;
        #[cfg(windows)]
        pub fn share_mode(&mut self, val: u32) -> &mut Self;
        #[cfg(windows)]
        pub fn custom_flags(&mut self, flags: u32) -> &mut Self;
        #[cfg(windows)]
        pub fn attributes(&mut self, val: u32) -> &mut Self;
        #[cfg(windows)]
        pub fn security_qos_flags(&mut self, flags: u32) -> &mut Self;
    }
    impl Clone for OpenOptions {
        fn clone(&self) -> Self;
    }
}
