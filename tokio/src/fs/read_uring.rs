use crate::fs::OpenOptions;
use crate::runtime::driver::op::Op;

use std::io::ErrorKind;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::{cmp, io};

// this algorithm is inspired from rust std lib version 1.90.0
// https://doc.rust-lang.org/1.90.0/src/std/io/mod.rs.html#409
const PROBE_SIZE: usize = 32;
const PROBE_SIZE_U32: u32 = PROBE_SIZE as u32;

// Max bytes we can read using io uring submission at a time
// SAFETY: cannot be higher than u32::MAX for safe cast
const MAX_READ_SIZE: usize = u32::MAX as usize;

pub(crate) async fn read_uring(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new().read(true).open(path).await?;

    // TODO: use io uring in the future to obtain metadata
    let size_hint: Option<usize> = file.metadata().await.map(|m| m.len() as usize).ok();

    let fd: OwnedFd = file
        .try_into_std()
        .expect("unexpected in-flight operation detected")
        .into();

    // extra single capacity for the whole size to fit without any reallocation
    let buf = Vec::with_capacity(size_hint.unwrap_or(0).saturating_add(1));

    read_to_end_uring(size_hint, fd, buf).await
}

async fn read_to_end_uring(
    size_hint: Option<usize>,
    mut fd: OwnedFd,
    mut buf: Vec<u8>,
) -> io::Result<Vec<u8>> {
    let mut offset = 0;
    let mut consecutive_short_reads = 0;

    let start_cap = buf.capacity();

    // if buffer has no room and no size_hint, start with a small probe_read from 0 offset
    if (size_hint.is_none() || size_hint == Some(0)) && buf.capacity() - buf.len() < (PROBE_SIZE) {
        let (size_read, r_fd, r_buf) = small_probe_read(fd, buf, offset).await?;

        if size_read == 0 {
            return Ok(r_buf);
        }

        buf = r_buf;
        fd = r_fd;
        offset += size_read as u64;
    }

    loop {
        if buf.len() == buf.capacity() && buf.capacity() == start_cap {
            // The buffer might be an exact fit. Let's read into a probe buffer
            // and see if it returns `Ok(0)`. If so, we've avoided an
            // unnecessary increasing of the capacity. But if not, append the
            // probe buffer to the primary buffer and let its capacity grow.
            let (size_read, r_fd, r_buf) = small_probe_read(fd, buf, offset).await?;

            if size_read == 0 {
                return Ok(r_buf);
            }

            buf = r_buf;
            fd = r_fd;
            offset += size_read as u64;
        }

        // buf is full, need more capacity
        if buf.len() == buf.capacity() {
            if consecutive_short_reads > 1 {
                buf.try_reserve(PROBE_SIZE.saturating_mul(consecutive_short_reads))?;
            } else {
                buf.try_reserve(PROBE_SIZE)?;
            }
        }

        // doesn't matter if we have a valid size_hint or not, if we do more
        // than 2 consecutive_short_reads, gradually increase the buffer
        // capacity to read more data at a time

        // prepare the spare capacity to be read into
        let spare = buf.capacity() - buf.len();
        let buf_len = cmp::min(spare, MAX_READ_SIZE);

        // SAFETY: buf_len cannot be greater than u32::MAX because max_read_size
        // is u32::MAX
        let mut read_len = buf_len as u32;

        loop {
            // read into spare capacity
            let (res, r_fd, mut r_buf) = Op::read(fd, buf, read_len, offset).await;

            match res {
                Ok(0) => return Ok(r_buf),
                Ok(size_read) => {
                    let new_len = size_read as usize + r_buf.len();
                    // SAFETY: We didn't read more than what as reserved
                    // as capacity, the _size_read was initialized by the kernel
                    // via a mutable pointer
                    unsafe { r_buf.set_len(new_len) }

                    let requested = read_len;

                    fd = r_fd;
                    buf = r_buf;
                    offset += size_read as u64;
                    read_len -= size_read;

                    // 1. In case of no size_hint and a large file, if we keep reading
                    // PROBE_SIZE, we want to increment number of short reads in order to gradually
                    // increase read size per Op submission
                    // 2. In case of small reads by the kernel, also gradually increase
                    // read size per Op submission to read files in lesser cycles
                    if size_read <= requested {
                        consecutive_short_reads += 1;
                    } else {
                        consecutive_short_reads = 0;
                    }

                    // keep reading if there's something left to be read
                    if read_len > 0 {
                        continue;
                    } else {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    buf = r_buf;
                    fd = r_fd;

                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

async fn small_probe_read(
    mut fd: OwnedFd,
    mut buf: Vec<u8>,
    offset: u64,
) -> io::Result<(u32, OwnedFd, Vec<u8>)> {
    // don't reserve more than PROBE_SIZE or double the capacity using
    // try_reserve beacuse we'll be reading only PROBE_SIZE length
    buf.reserve_exact(PROBE_SIZE);

    loop {
        let (res, r_fd, mut r_buf) = Op::read(fd, buf, PROBE_SIZE_U32, offset).await;

        match res {
            Ok(size_read) => {
                let size_read_usize = size_read as usize;

                let new_len = size_read_usize + r_buf.len();
                // SAFETY: We didn't read more than what as reserved
                // as capacity, the _size_read was initialized by the kernel
                // via a mutable pointer
                unsafe { r_buf.set_len(new_len) }

                return Ok((size_read, r_fd, r_buf));
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {
                buf = r_buf;
                fd = r_fd;

                continue;
            }
            Err(e) => return Err(e),
        }
    }
}
