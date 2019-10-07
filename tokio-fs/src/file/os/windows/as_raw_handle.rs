use std::os::windows::io::{AsRawHandle, RawHandle};

use crate::File;

impl AsRawHandle for File {
    fn as_raw_handle(&self) -> RawHandle {
        self.std.as_raw_handle()
    }
}
