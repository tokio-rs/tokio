#![allow(clippy::unnecessary_operation)]

use std::collections::VecDeque;
use std::fmt;
use std::fs::{Metadata, Permissions};
use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct File {
    shared: Arc<Mutex<Shared>>,
}

pub struct Handle {
    shared: Arc<Mutex<Shared>>,
}

struct Shared {
    calls: VecDeque<Call>,
}

#[derive(Debug)]
enum Call {
    Read(io::Result<Vec<u8>>),
    Write(io::Result<Vec<u8>>),
    Seek(SeekFrom, io::Result<u64>),
    SyncAll(io::Result<()>),
    SyncData(io::Result<()>),
    SetLen(u64, io::Result<()>),
}

impl Handle {
    pub fn read(&self, data: &[u8]) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls.push_back(Call::Read(Ok(data.to_owned())));
        self
    }

    pub fn read_err(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::Read(Err(io::ErrorKind::Other.into())));
        self
    }

    pub fn write(&self, data: &[u8]) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls.push_back(Call::Write(Ok(data.to_owned())));
        self
    }

    pub fn write_err(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::Write(Err(io::ErrorKind::Other.into())));
        self
    }

    pub fn seek_start_ok(&self, offset: u64) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::Seek(SeekFrom::Start(offset), Ok(offset)));
        self
    }

    pub fn seek_current_ok(&self, offset: i64, ret: u64) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::Seek(SeekFrom::Current(offset), Ok(ret)));
        self
    }

    pub fn sync_all(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls.push_back(Call::SyncAll(Ok(())));
        self
    }

    pub fn sync_all_err(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::SyncAll(Err(io::ErrorKind::Other.into())));
        self
    }

    pub fn sync_data(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls.push_back(Call::SyncData(Ok(())));
        self
    }

    pub fn sync_data_err(&self) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::SyncData(Err(io::ErrorKind::Other.into())));
        self
    }

    pub fn set_len(&self, size: u64) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls.push_back(Call::SetLen(size, Ok(())));
        self
    }

    pub fn set_len_err(&self, size: u64) -> &Self {
        let mut s = self.shared.lock().unwrap();
        s.calls
            .push_back(Call::SetLen(size, Err(io::ErrorKind::Other.into())));
        self
    }

    pub fn remaining(&self) -> usize {
        let s = self.shared.lock().unwrap();
        s.calls.len()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let s = self.shared.lock().unwrap();
            assert_eq!(0, s.calls.len());
        }
    }
}

impl File {
    pub fn open(_: PathBuf) -> io::Result<File> {
        unimplemented!();
    }

    pub fn create(_: PathBuf) -> io::Result<File> {
        unimplemented!();
    }

    pub fn mock() -> (Handle, File) {
        let shared = Arc::new(Mutex::new(Shared {
            calls: VecDeque::new(),
        }));

        let handle = Handle {
            shared: shared.clone(),
        };
        let file = File { shared };

        (handle, file)
    }

    pub fn sync_all(&self) -> io::Result<()> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(SyncAll(ret)) => ret,
            Some(op) => panic!("expected next call to be {:?}; was sync_all", op),
            None => panic!("did not expect call"),
        }
    }

    pub fn sync_data(&self) -> io::Result<()> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(SyncData(ret)) => ret,
            Some(op) => panic!("expected next call to be {:?}; was sync_all", op),
            None => panic!("did not expect call"),
        }
    }

    pub fn set_len(&self, size: u64) -> io::Result<()> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(SetLen(arg, ret)) => {
                assert_eq!(arg, size);
                ret
            }
            Some(op) => panic!("expected next call to be {:?}; was sync_all", op),
            None => panic!("did not expect call"),
        }
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        unimplemented!();
    }

    pub fn set_permissions(&self, _perm: Permissions) -> io::Result<()> {
        unimplemented!();
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        unimplemented!();
    }
}

impl Read for &'_ File {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(Read(Ok(data))) => {
                assert!(dst.len() >= data.len());
                assert!(dst.len() <= 16 * 1024, "actual = {}", dst.len()); // max buffer

                &mut dst[..data.len()].copy_from_slice(&data);
                Ok(data.len())
            }
            Some(Read(Err(e))) => Err(e),
            Some(op) => panic!("expected next call to be {:?}; was a read", op),
            None => panic!("did not expect call"),
        }
    }
}

impl Write for &'_ File {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(Write(Ok(data))) => {
                assert_eq!(src, &data[..]);
                Ok(src.len())
            }
            Some(Write(Err(e))) => Err(e),
            Some(op) => panic!("expected next call to be {:?}; was write", op),
            None => panic!("did not expect call"),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for &'_ File {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        use self::Call::*;

        let mut s = self.shared.lock().unwrap();

        match s.calls.pop_front() {
            Some(Seek(expect, res)) => {
                assert_eq!(expect, pos);
                res
            }
            Some(op) => panic!("expected call {:?}; was `seek`", op),
            None => panic!("did not expect call; was `seek`"),
        }
    }
}

impl fmt::Debug for File {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("mock::File").finish()
    }
}

#[cfg(unix)]
impl std::os::unix::io::AsRawFd for File {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        unimplemented!();
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for File {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        unimplemented!();
    }
}
